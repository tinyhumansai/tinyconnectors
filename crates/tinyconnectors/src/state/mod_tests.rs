//! Unit tests for the document-backed preference store and the declared storage grant.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use serde_json::json;
use tinyconnectors_sync::{Error, PREFS_NAMESPACE, PrefsStore, ScopePrefStore, UserScopePref};
use tinystoragedrivers_core::{DocumentStore, MemoryStorage, Scope, StorageBackend};

use super::{DocumentPrefsStore, STORAGE_NEEDS};

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
async fn a_document_without_the_value_field_is_an_error_not_unset() {
    let (documents, store) = store();
    documents
        .put(
            PREFS_NAMESPACE,
            "gmail",
            json!({ "other": 1 }),
            tinystoragedrivers_core::Precondition::None,
        )
        .await
        .unwrap();
    assert!(matches!(store.get("gmail").await, Err(Error::Store { .. })));
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

    assert_eq!(PREFS_NAMESPACE, "composio-user-scopes");
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

#[tokio::test]
async fn declared_storage_needs_cover_every_persisted_resource() {
    // This is the exact schema the host uses when deciding which storage to
    // grant. Preferences are one document per toolkit with no queried fields;
    // trigger history is append-only under its daily stream prefix.
    let needs = &*STORAGE_NEEDS;
    assert_eq!(needs.collections.len(), 1);
    let collection = &needs.collections[0];
    assert_eq!(collection.name, PREFS_NAMESPACE);
    assert!(collection.indexes.is_empty());
    assert!(collection.ttl_field.is_none());
    assert!(collection.search.is_none());
    assert!(needs.blob_prefixes.is_empty());
    assert_eq!(needs.stream_prefixes, &["triggers/"]);
    assert!(needs.secret_prefixes.is_empty());

    let storage = scoped("declared-needs");
    let documents = storage.documents().clone();
    let preferences = DocumentPrefsStore::new(documents.clone());
    preferences
        .set("gmail", &json!({ "read": true }))
        .await
        .unwrap();
    let saved = documents
        .get(&collection.name, "gmail")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved.doc["value"]["read"], true);

    let streams = storage.streams().clone();
    let archive = crate::triggers::TriggerArchive::new(streams.clone());
    archive
        .record(
            "gmail",
            "trigger",
            "event-1",
            "uuid-1",
            &json!({ "ok": true }),
        )
        .await
        .unwrap();
    let names = streams.streams(needs.stream_prefixes[0]).await.unwrap();
    assert_eq!(names.len(), 1);
    assert!(names[0].starts_with(needs.stream_prefixes[0]));
}
