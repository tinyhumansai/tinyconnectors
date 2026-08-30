//! Unit tests for the file-backed state store.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use serde_json::json;
use tinyconnectors_sync::{STATE_NAMESPACE, SyncState, SyncStateStore};

use super::FileStateStore;

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "tinyconnectors-state-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("scratch directory");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn a_key_that_was_never_written_reads_as_absent() {
    // The normal first case for a connection that has never synced — not a
    // failure, or every first run would report one.
    let dir = TempDir::new("absent");
    let store = FileStateStore::new(&dir.0);
    assert!(
        store
            .get(STATE_NAMESPACE, "gmail:conn_1")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn a_value_round_trips() {
    let dir = TempDir::new("roundtrip");
    let store = FileStateStore::new(&dir.0);

    store
        .set(
            STATE_NAMESPACE,
            "gmail:conn_1",
            &json!({ "cursor": "page-2" }),
        )
        .await
        .unwrap();

    let value = store.get(STATE_NAMESPACE, "gmail:conn_1").await.unwrap();
    assert_eq!(value.unwrap()["cursor"], "page-2");
}

#[tokio::test]
async fn sync_state_persists_through_it() {
    let dir = TempDir::new("syncstate");
    let store = FileStateStore::new(&dir.0);

    let mut state = SyncState::new("gmail", "conn_1");
    state.cursor = Some("page-3".into());
    state.mark_synced("m1", Some("v1"));
    state.save(&store).await.unwrap();

    let loaded = SyncState::load(&store, "gmail", "conn_1").await.unwrap();
    assert_eq!(loaded.cursor.as_deref(), Some("page-3"));
    assert!(loaded.is_synced("m1"));
}

#[tokio::test]
async fn two_connections_do_not_share_a_file() {
    // One connection's progress must never suppress another's.
    let dir = TempDir::new("separate");
    let store = FileStateStore::new(&dir.0);

    store
        .set(STATE_NAMESPACE, "gmail:conn_1", &json!({ "cursor": "a" }))
        .await
        .unwrap();
    store
        .set(STATE_NAMESPACE, "gmail:conn_2", &json!({ "cursor": "b" }))
        .await
        .unwrap();

    assert_eq!(
        store
            .get(STATE_NAMESPACE, "gmail:conn_1")
            .await
            .unwrap()
            .unwrap()["cursor"],
        "a"
    );
    assert_eq!(
        store
            .get(STATE_NAMESPACE, "gmail:conn_2")
            .await
            .unwrap()
            .unwrap()["cursor"],
        "b"
    );
}

#[tokio::test]
async fn a_key_cannot_escape_the_state_directory() {
    // Keys are `toolkit:connection_id`, and the connection id came from a
    // backend response. Without sanitizing, one containing `../` would write
    // wherever it liked.
    let dir = TempDir::new("traversal");
    let store = FileStateStore::new(&dir.0);

    store
        .set(STATE_NAMESPACE, "../../escaped", &json!({ "leaked": true }))
        .await
        .unwrap();

    assert!(
        !dir.0.parent().unwrap().join("escaped.json").exists(),
        "the write must not land outside the state directory"
    );
    // And it still round-trips under its sanitized name.
    let value = store.get(STATE_NAMESPACE, "../../escaped").await.unwrap();
    assert_eq!(value.unwrap()["leaked"], true);
}

#[tokio::test]
async fn a_namespace_cannot_escape_either() {
    let dir = TempDir::new("nstraversal");
    let store = FileStateStore::new(&dir.0);

    store.set("../evil", "k", &json!({})).await.unwrap();
    assert!(!dir.0.parent().unwrap().join("evil").exists());
}

#[tokio::test]
async fn a_lone_dot_key_does_not_become_a_directory_reference() {
    let dir = TempDir::new("dots");
    let store = FileStateStore::new(&dir.0);

    for key in [".", "..", "..."] {
        store
            .set(STATE_NAMESPACE, key, &json!({ "k": key }))
            .await
            .unwrap();
        assert!(store.get(STATE_NAMESPACE, key).await.unwrap().is_some());
    }
}

#[tokio::test]
async fn a_corrupt_file_is_reported_rather_than_read_as_absent() {
    // Reading it as absent would silently restart the connection's history,
    // re-ingesting everything the user already had.
    let dir = TempDir::new("corrupt");
    let store = FileStateStore::new(&dir.0);
    store
        .set(STATE_NAMESPACE, "gmail:conn_1", &json!({}))
        .await
        .unwrap();

    let path = dir.0.join(STATE_NAMESPACE).join("gmail_conn_1.json");
    fs::write(&path, "{ not json").unwrap();

    assert!(store.get(STATE_NAMESPACE, "gmail:conn_1").await.is_err());
}

#[tokio::test]
async fn a_write_leaves_no_temporary_file_behind() {
    let dir = TempDir::new("atomic");
    let store = FileStateStore::new(&dir.0);
    store
        .set(STATE_NAMESPACE, "gmail:conn_1", &json!({ "cursor": "a" }))
        .await
        .unwrap();

    let namespace_dir = dir.0.join(STATE_NAMESPACE);
    let stray: Vec<_> = fs::read_dir(&namespace_dir)
        .unwrap()
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.path().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(stray.is_empty(), "the rename must replace, not accumulate");
}
