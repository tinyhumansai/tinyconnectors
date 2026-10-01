//! Unit tests for the per-toolkit scope preference.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::json;

use super::{PREFS_NAMESPACE, UserScopePref};
use crate::scope::ToolScope;
use crate::state::{STATE_NAMESPACE, SyncStateStore};
use crate::{Error, Result};

#[derive(Debug, Default)]
struct MemoryStore {
    values: Mutex<HashMap<(String, String), serde_json::Value>>,
}

#[async_trait]
impl SyncStateStore for MemoryStore {
    async fn get(&self, namespace: &str, key: &str) -> Result<Option<serde_json::Value>> {
        Ok(self
            .values
            .lock()
            .unwrap()
            .get(&(namespace.to_string(), key.to_string()))
            .cloned())
    }
    async fn set(&self, namespace: &str, key: &str, value: &serde_json::Value) -> Result<()> {
        self.values
            .lock()
            .unwrap()
            .insert((namespace.to_string(), key.to_string()), value.clone());
        Ok(())
    }
}

#[test]
fn the_default_allows_reads_and_writes_but_not_admin() {
    // Read alone makes most integrations useless. Admin is the set that
    // destroys things, so a user who wants it should have to say so.
    let pref = UserScopePref::default();
    assert!(pref.allows(ToolScope::Read));
    assert!(pref.allows(ToolScope::Write));
    assert!(!pref.allows(ToolScope::Admin));
}

#[test]
fn the_three_flags_are_independent() {
    // A threshold cannot express "read and delete stale mail, but never send".
    let pref = UserScopePref {
        read: true,
        write: false,
        admin: true,
    };
    assert!(pref.allows(ToolScope::Read));
    assert!(!pref.allows(ToolScope::Write));
    assert!(pref.allows(ToolScope::Admin));
}

#[test]
fn the_key_is_normalized() {
    // A key one caller spelled differently would not fail — it would read as
    // "no preference" and hand the agent the default while the user's saved
    // choice sat one key away.
    for spelling in ["GMAIL", " Gmail ", "gmail"] {
        assert_eq!(UserScopePref::key(spelling), "gmail");
    }
}

#[test]
fn the_namespace_is_distinct_from_sync_state() {
    // A preference and a cursor for one toolkit must never collide.
    assert_ne!(PREFS_NAMESPACE, STATE_NAMESPACE);
    assert_eq!(PREFS_NAMESPACE, "composio-user-scopes");
}

#[test]
fn a_partial_row_fills_in_the_defaults() {
    // Rows written before a flag existed must not read as denying it.
    let pref: UserScopePref = serde_json::from_value(json!({ "admin": true })).unwrap();
    assert!(pref.read);
    assert!(pref.write);
    assert!(pref.admin);
}

#[tokio::test]
async fn a_toolkit_with_no_stored_preference_gets_the_default() {
    let store = MemoryStore::default();
    let pref = UserScopePref::load(&store, "gmail").await.unwrap();
    assert_eq!(pref, UserScopePref::default());
}

#[tokio::test]
async fn a_preference_round_trips() {
    let store = MemoryStore::default();
    let saved = UserScopePref {
        read: true,
        write: false,
        admin: false,
    };
    saved.save(&store, "Gmail").await.unwrap();

    // Saved under one spelling, read under another.
    let loaded = UserScopePref::load(&store, "  gmail ").await.unwrap();
    assert_eq!(loaded, saved);
}

#[tokio::test]
async fn an_unreadable_preference_is_an_error_not_a_grant() {
    // Falling back to the default here would quietly hand the agent write
    // permission the user may have explicitly removed.
    let store = MemoryStore::default();
    store
        .set(PREFS_NAMESPACE, "gmail", &json!("not an object"))
        .await
        .unwrap();

    let error = UserScopePref::load(&store, "gmail").await.unwrap_err();
    assert!(matches!(error, Error::Decode { .. }));
}

#[tokio::test]
async fn preferences_do_not_collide_between_toolkits() {
    let store = MemoryStore::default();
    UserScopePref {
        read: true,
        write: false,
        admin: false,
    }
    .save(&store, "gmail")
    .await
    .unwrap();

    assert_eq!(
        UserScopePref::load(&store, "notion").await.unwrap(),
        UserScopePref::default()
    );
}

#[tokio::test]
async fn a_preference_lands_under_the_prefs_namespace() {
    // Not the sync-state namespace: a cursor and a preference for one toolkit
    // must never collide.
    let store = MemoryStore::default();
    UserScopePref::default()
        .save(&store, "gmail")
        .await
        .unwrap();

    let values = store.values.lock().unwrap();
    assert!(values.contains_key(&(PREFS_NAMESPACE.to_string(), "gmail".to_string())));
    assert!(!values.contains_key(&(STATE_NAMESPACE.to_string(), "gmail".to_string())));
}

#[test]
fn an_empty_toolkit_still_produces_a_usable_key() {
    // Reached only through a member that already rejects a blank toolkit, but
    // handled rather than assumed away: a future caller need not come that way.
    assert_eq!(UserScopePref::key("   "), "");
}

#[test]
fn every_scope_is_checked_against_its_own_flag() {
    let none = UserScopePref {
        read: false,
        write: false,
        admin: false,
    };
    for scope in [ToolScope::Read, ToolScope::Write, ToolScope::Admin] {
        assert!(!none.allows(scope), "{scope:?}");
    }

    let all = UserScopePref {
        read: true,
        write: true,
        admin: true,
    };
    for scope in [ToolScope::Read, ToolScope::Write, ToolScope::Admin] {
        assert!(all.allows(scope), "{scope:?}");
    }
}
