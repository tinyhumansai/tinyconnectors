//! Unit tests for the document-backed preference store and the declared storage grant.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use serde_json::json;
use tinyconnectors_sync::{Error, PREFS_NAMESPACE, PrefsStore, ScopePrefStore, UserScopePref};
use tinystoragedrivers_core::{DocumentStore, MemoryStorage, Scope, StorageBackend};

use super::{DocumentPrefsStore, storage_needs};

fn scoped(name: &str) -> tinystoragedrivers_core::ScopedStorage {
    MemoryStorage::new()
        .for_scope(&Scope::new(name).unwrap())
        .unwrap()
}

fn store() -> (Arc<dyn DocumentStore>, DocumentPrefsStore) {
    let documents = scoped("alice").documents().clone();
    (documents.clone(), DocumentPrefsStore::new(documents))
}

#[tokio::test]
async fn a_key_that_was_never_written_reads_as_absent() {
    // The normal case for a toolkit the user never configured, not a failure.
    let (_, store) = store();
    assert!(store.get("gmail").await.unwrap().is_none());
}

#[tokio::test]
async fn a_value_round_trips_even_when_it_is_not_an_object() {
    let (_, store) = store();
    store.set("gmail", &json!({ "read": true })).await.unwrap();
    store.set("slack", &json!([1, 2])).await.unwrap();

    assert_eq!(store.get("gmail").await.unwrap().unwrap()["read"], true);
    assert_eq!(store.get("slack").await.unwrap().unwrap(), json!([1, 2]));
}

#[tokio::test]
async fn setting_a_key_again_replaces_the_value() {
    let (_, store) = store();
    store.set("gmail", &json!({ "v": 1 })).await.unwrap();
    store.set("gmail", &json!({ "v": 2 })).await.unwrap();
    assert_eq!(store.get("gmail").await.unwrap().unwrap()["v"], 2);
}

#[tokio::test]
async fn a_scope_preference_persists_through_it() {
    let (documents, store) = store();
    let saved = UserScopePref {
        read: true,
        write: false,
        admin: false,
    };
    saved.save(&store, "Gmail").await.unwrap();

    // A second store over the same port sees it: the port is the record.
    let reopened = DocumentPrefsStore::new(documents);
    assert_eq!(
        UserScopePref::load(&reopened, "gmail").await.unwrap(),
        saved
    );
}

#[tokio::test]
async fn preferences_live_in_the_prefs_collection_one_document_per_toolkit() {
    let (documents, store) = store();
    store.set("gmail", &json!({ "v": "a" })).await.unwrap();
    store.set("slack", &json!({ "v": "b" })).await.unwrap();

    assert_eq!(PREFS_NAMESPACE, "connectors.user_scopes");
    assert!(
        documents
            .get(PREFS_NAMESPACE, "gmail")
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        documents
            .get(PREFS_NAMESPACE, "slack")
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(store.get("gmail").await.unwrap().unwrap()["v"], "a");
    assert_eq!(store.get("slack").await.unwrap().unwrap()["v"], "b");
}

#[tokio::test]
async fn two_users_do_not_see_each_others_preferences() {
    let backend = MemoryStorage::new();
    let alice = DocumentPrefsStore::new(
        backend
            .for_scope(&Scope::new("alice").unwrap())
            .unwrap()
            .documents()
            .clone(),
    );
    let bob = DocumentPrefsStore::new(
        backend
            .for_scope(&Scope::new("bob").unwrap())
            .unwrap()
            .documents()
            .clone(),
    );
    alice.set("gmail", &json!({ "v": 1 })).await.unwrap();
    assert!(bob.get("gmail").await.unwrap().is_none());
}

#[tokio::test]
async fn a_path_like_key_is_just_an_id() {
    // Keys were sanitized when they became file names; as document ids they
    // carry no path meaning and round-trip verbatim.
    let (_, store) = store();
    store
        .set("../../escaped", &json!({ "leaked": true }))
        .await
        .unwrap();
    assert_eq!(
        store.get("../../escaped").await.unwrap().unwrap()["leaked"],
        true
    );
}

#[tokio::test]
async fn a_rejected_key_is_reported_rather_than_swallowed() {
    // A preference that silently fails to save would revert with no explanation.
    let (_, store) = store();
    assert!(matches!(
        store.set("", &json!({})).await.unwrap_err(),
        Error::Store { .. }
    ));
    assert!(matches!(
        store.get("").await.unwrap_err(),
        Error::Store { .. }
    ));
}

#[test]
fn the_declared_grant_matches_what_the_stores_write() {
    // The host generates its grant from the schema; if the stores moved to
    // another collection or stream prefix without it, every call would be
    // refused by the grant.
    let needs = storage_needs();
    assert_eq!(needs.collections, [PREFS_NAMESPACE]);
    assert_eq!(needs.stream_prefixes, ["connectors.triggers/"]);
    let archive = crate::triggers::TriggerArchive::new(scoped("grant").streams().clone());
    assert!(
        archive
            .current_day_file()
            .starts_with(needs.stream_prefixes[0].as_str())
    );
}

#[tokio::test]
async fn a_preference_is_stored_as_the_bare_object_with_its_schema_version() {
    let (documents, store) = store();
    store.set("gmail", &json!({ "read": true })).await.unwrap();
    let found = documents
        .get(PREFS_NAMESPACE, "gmail")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.doc, json!({ "read": true }));
    assert_eq!(found.meta.schema, 2);
}
