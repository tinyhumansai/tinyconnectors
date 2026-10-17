//! A migrated store reads back through the module's own preference store and
//! trigger archive. The steps themselves are tested in `tinyconnectors-bus`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use serde_json::{Value, json};
use tinyconnectors_bus::storage::{LEGACY_PREFS_NAMESPACE, PREFS_NAMESPACE, storage_schema};
use tinyconnectors_bus::vocabulary::UserScopePref;
use tinyconnectors_sync::{PrefsStore, ScopePrefStore};
use tinystoragedrivers_core::schema::{MigrateOptions, Registry};
use tinystoragedrivers_core::{MemoryStorage, Precondition, Scope, ScopedStorage, StorageBackend};

use crate::state::DocumentPrefsStore;
use crate::triggers::TriggerArchive;

fn module_storage(backend: &MemoryStorage) -> ScopedStorage {
    backend
        .for_scope(&Scope::local().module("connectors").unwrap())
        .unwrap()
}

async fn migrated(storage: &ScopedStorage, backend: &MemoryStorage) {
    let registry = Registry::compose([storage_schema()]).unwrap();
    registry.prepare(backend).await.unwrap();
    registry
        .migrate(storage, &MigrateOptions::new())
        .await
        .unwrap();
}

async fn seed_v1(storage: &ScopedStorage, id: &str, body: Value) {
    storage
        .documents()
        .put(LEGACY_PREFS_NAMESPACE, id, body, Precondition::None)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_migrated_preference_reads_through_the_store_with_its_choice_intact() {
    let backend = MemoryStorage::new();
    let storage = module_storage(&backend);
    // A denied write must stay denied: it is the case a lossy step would flip.
    seed_v1(
        &storage,
        "slack",
        json!({ "value": { "read": true, "write": false, "admin": false } }),
    )
    .await;
    migrated(&storage, &backend).await;

    let store = DocumentPrefsStore::new(Arc::clone(storage.documents()));
    let pref = UserScopePref::load(&store, "slack").await.unwrap();
    assert!(pref.read);
    assert!(!pref.write);
}

#[tokio::test]
async fn a_non_object_preference_moves_under_a_named_field_and_reads_back_whole() {
    let backend = MemoryStorage::new();
    let storage = module_storage(&backend);
    seed_v1(&storage, "slack", json!({ "value": [1, 2] })).await;
    seed_v1(&storage, "notion", json!({ "value": "x" })).await;
    migrated(&storage, &backend).await;

    let found = storage
        .documents()
        .get(PREFS_NAMESPACE, "slack")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.doc, json!({ "non_object_value": [1, 2] }));

    let store = DocumentPrefsStore::new(Arc::clone(storage.documents()));
    assert_eq!(store.get("slack").await.unwrap().unwrap(), json!([1, 2]));
    assert_eq!(store.get("notion").await.unwrap().unwrap(), json!("x"));
}

#[tokio::test]
async fn an_object_preference_that_uses_the_reserved_field_is_not_confused_with_a_wrapper() {
    let backend = MemoryStorage::new();
    let storage = module_storage(&backend);
    let tricky = json!({ "non_object_value": 7 });
    seed_v1(&storage, "gmail", json!({ "value": tricky })).await;
    migrated(&storage, &backend).await;

    let store = DocumentPrefsStore::new(Arc::clone(storage.documents()));
    assert_eq!(store.get("gmail").await.unwrap().unwrap(), tricky);
}

#[tokio::test]
async fn a_v1_document_without_a_value_stays_an_unreadable_preference_not_a_default() {
    // Defaulting it would silently re-enable a write the user denied.
    let backend = MemoryStorage::new();
    let storage = module_storage(&backend);
    seed_v1(&storage, "gmail", json!({ "other": 1 })).await;
    migrated(&storage, &backend).await;

    let store = DocumentPrefsStore::new(Arc::clone(storage.documents()));
    let loaded = UserScopePref::load(&store, "gmail").await;
    assert!(loaded.is_err(), "a corrupt row must not read as defaults");
}

const LEGACY_STREAM: &str = "triggers/2026-10-15";

#[tokio::test]
async fn migrated_trigger_history_reads_back_through_the_archive() {
    let backend = MemoryStorage::new();
    let storage = module_storage(&backend);
    let entry = |id: &str| {
        json!({
            "received_at_ms": 1_760_000_000_000_u64,
            "toolkit": "gmail",
            "trigger": "GMAIL_NEW_GMAIL_MESSAGE",
            "metadata_id": id,
            "metadata_uuid": "u",
            "payload": { "subject": "hi" },
        })
    };
    for id in ["evt-1", "evt-2"] {
        storage
            .streams()
            .append(LEGACY_STREAM, entry(id))
            .await
            .unwrap();
    }
    migrated(&storage, &backend).await;

    // The archive reads the migrated history back through its public API.
    let history = TriggerArchive::new(Arc::clone(storage.streams()))
        .list_recent(None)
        .await
        .unwrap();
    let ids: Vec<&str> = history
        .entries
        .iter()
        .map(|entry| entry.metadata_id.as_str())
        .collect();
    assert_eq!(ids, ["evt-2", "evt-1"]);
}
