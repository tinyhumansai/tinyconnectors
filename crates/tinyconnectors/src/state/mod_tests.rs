//! Unit tests for the file-backed preference store.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use serde_json::json;
use tinyconnectors_sync::{Error, PREFS_NAMESPACE, PrefsStore, UserScopePref};

use super::FilePrefsStore;

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
    // The normal case for a toolkit the user never configured — not a failure,
    // or every first read would report one.
    let dir = TempDir::new("absent");
    let store = FilePrefsStore::new(&dir.0);
    assert!(store.get("gmail").await.unwrap().is_none());
}

#[tokio::test]
async fn a_value_round_trips() {
    let dir = TempDir::new("roundtrip");
    let store = FilePrefsStore::new(&dir.0);

    store.set("gmail", &json!({ "read": true })).await.unwrap();

    let value = store.get("gmail").await.unwrap();
    assert_eq!(value.unwrap()["read"], true);
}

#[tokio::test]
async fn a_scope_preference_persists_through_it() {
    let dir = TempDir::new("pref");
    let store = FilePrefsStore::new(&dir.0);

    let saved = UserScopePref {
        read: true,
        write: false,
        admin: false,
    };
    saved.save(&store, "Gmail").await.unwrap();

    // A second store over the same directory sees it: the file is the record.
    let reopened = FilePrefsStore::new(&dir.0);
    assert_eq!(
        UserScopePref::load(&reopened, "gmail").await.unwrap(),
        saved
    );
}

#[tokio::test]
async fn the_file_layout_is_unchanged_so_saved_preferences_keep_reading() {
    // `<state_dir>/composio-user-scopes/<toolkit>.json` is what earlier
    // releases wrote; moving it would silently reset every user's choices.
    let dir = TempDir::new("layout");
    let store = FilePrefsStore::new(&dir.0);
    store.set("gmail", &json!({})).await.unwrap();

    assert!(
        dir.0
            .join("composio-user-scopes")
            .join("gmail.json")
            .is_file()
    );
    assert_eq!(PREFS_NAMESPACE, "composio-user-scopes");
}

#[tokio::test]
async fn two_toolkits_do_not_share_a_file() {
    let dir = TempDir::new("separate");
    let store = FilePrefsStore::new(&dir.0);

    store.set("gmail", &json!({ "v": "a" })).await.unwrap();
    store.set("slack", &json!({ "v": "b" })).await.unwrap();

    assert_eq!(store.get("gmail").await.unwrap().unwrap()["v"], "a");
    assert_eq!(store.get("slack").await.unwrap().unwrap()["v"], "b");
}

#[tokio::test]
async fn a_key_cannot_escape_the_state_directory() {
    // Without sanitizing, a key containing `../` would write wherever it liked.
    let dir = TempDir::new("traversal");
    let store = FilePrefsStore::new(&dir.0);

    store
        .set("../../escaped", &json!({ "leaked": true }))
        .await
        .unwrap();

    assert!(
        !dir.0.parent().unwrap().join("escaped.json").exists(),
        "the write must not land outside the state directory"
    );
    // And it still round-trips under its sanitized name.
    let value = store.get("../../escaped").await.unwrap();
    assert_eq!(value.unwrap()["leaked"], true);
}

#[tokio::test]
async fn a_lone_dot_key_does_not_become_a_directory_reference() {
    let dir = TempDir::new("dots");
    let store = FilePrefsStore::new(&dir.0);

    for key in [".", "..", "..."] {
        store.set(key, &json!({ "k": key })).await.unwrap();
        assert!(store.get(key).await.unwrap().is_some());
    }
}

#[tokio::test]
async fn a_corrupt_file_is_reported_rather_than_read_as_absent() {
    // Reading it as absent would silently hand the agent the default scopes
    // while the user's saved choice sat unreadable on disk.
    let dir = TempDir::new("corrupt");
    let store = FilePrefsStore::new(&dir.0);
    store.set("gmail", &json!({})).await.unwrap();

    let path = dir.0.join(PREFS_NAMESPACE).join("gmail.json");
    fs::write(&path, "{ not json").unwrap();

    assert!(store.get("gmail").await.is_err());
}

#[tokio::test]
async fn a_write_leaves_no_temporary_file_behind() {
    let dir = TempDir::new("atomic");
    let store = FilePrefsStore::new(&dir.0);
    store.set("gmail", &json!({ "read": true })).await.unwrap();

    let stray: Vec<_> = fs::read_dir(dir.0.join(PREFS_NAMESPACE))
        .unwrap()
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.path().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(stray.is_empty(), "the rename must replace, not accumulate");
}

#[tokio::test]
async fn a_write_creates_the_namespace_directory() {
    // The first write has nowhere to land otherwise.
    let dir = TempDir::new("mkdir");
    let store = FilePrefsStore::new(&dir.0);
    assert!(!dir.0.join(PREFS_NAMESPACE).exists());

    store.set("gmail", &json!({})).await.unwrap();
    assert!(dir.0.join(PREFS_NAMESPACE).is_dir());
}

#[tokio::test]
async fn a_write_into_an_unwritable_root_is_reported() {
    // Reported rather than swallowed: a preference that silently fails to save
    // would revert at the next restart with no explanation.
    let store = FilePrefsStore::new(std::path::Path::new("/proc/nonexistent-for-tests"));
    let error = store.set("gmail", &json!({})).await.unwrap_err();
    assert!(matches!(error, Error::Store { .. }));
}

#[tokio::test]
async fn a_read_of_an_unreadable_path_is_reported() {
    // A directory where a file should be: not "absent", which would reset the
    // user's choices to the default.
    let dir = TempDir::new("unreadable");
    let store = FilePrefsStore::new(&dir.0);
    fs::create_dir_all(dir.0.join(PREFS_NAMESPACE).join("gmail.json")).unwrap();

    assert!(store.get("gmail").await.is_err());
}
