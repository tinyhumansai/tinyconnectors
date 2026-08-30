//! Unit tests for sync state and the daily budget.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::json;

use super::{DEFAULT_DAILY_REQUEST_LIMIT, DailyBudget, STATE_NAMESPACE, SyncState, SyncStateStore};
use crate::{Error, Result};

#[derive(Debug, Default)]
struct MemoryStore {
    values: Mutex<HashMap<(String, String), serde_json::Value>>,
    fail: Mutex<Option<String>>,
}

#[async_trait]
impl SyncStateStore for MemoryStore {
    async fn get(&self, namespace: &str, key: &str) -> Result<Option<serde_json::Value>> {
        if let Some(message) = self.fail.lock().unwrap().clone() {
            return Err(Error::Store {
                key: key.to_string(),
                message,
            });
        }
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
fn the_state_namespace_is_pinned() {
    // Durable: changing it strands every user's cursor, and every connection
    // re-reads its whole history on the next run.
    assert_eq!(STATE_NAMESPACE, "composio-sync-state");
}

#[test]
fn the_key_is_the_toolkit_and_connection() {
    // Two connections to the same toolkit must not share a cursor, or one
    // account's progress silently suppresses the other's.
    assert_eq!(SyncState::key("gmail", "conn_1"), "gmail:conn_1");
    assert_ne!(
        SyncState::key("gmail", "conn_1"),
        SyncState::key("gmail", "conn_2")
    );
}

#[test]
fn a_fresh_budget_has_the_whole_allowance() {
    let budget = DailyBudget::default();
    assert_eq!(budget.remaining(), DEFAULT_DAILY_REQUEST_LIMIT);
    assert!(!budget.is_exhausted());
}

#[test]
fn spending_the_allowance_exhausts_the_budget() {
    let mut budget = DailyBudget::default();
    budget.record_requests(DEFAULT_DAILY_REQUEST_LIMIT);
    assert_eq!(budget.remaining(), 0);
    assert!(budget.is_exhausted());
}

#[test]
fn a_budget_from_an_earlier_day_has_rolled_over() {
    // Read-time rollover: a budget written yesterday and never saved back must
    // not keep suppressing runs today.
    let stale = DailyBudget {
        date: "2000-01-01".into(),
        requests_used: DEFAULT_DAILY_REQUEST_LIMIT,
        limit: DEFAULT_DAILY_REQUEST_LIMIT,
    };
    assert_eq!(stale.remaining(), DEFAULT_DAILY_REQUEST_LIMIT);
    assert!(!stale.is_exhausted());
}

#[test]
fn recording_against_a_stale_budget_starts_the_new_day_at_zero() {
    let mut budget = DailyBudget {
        date: "2000-01-01".into(),
        requests_used: 400,
        limit: DEFAULT_DAILY_REQUEST_LIMIT,
    };
    budget.record_requests(5);

    assert_ne!(budget.date, "2000-01-01");
    assert_eq!(budget.requests_used, 5, "yesterday's spend does not carry");
}

#[test]
fn an_overspending_run_stays_exhausted_rather_than_wrapping() {
    // Saturating, not wrapping: wrapping would hand a runaway loop a fresh
    // allowance at exactly the wrong moment.
    let mut budget = DailyBudget::default();
    budget.record_requests(u32::MAX);
    budget.record_requests(u32::MAX);
    assert!(budget.is_exhausted());
}

#[test]
fn an_action_costs_at_least_one_request() {
    // A retried action cost several requests; an action reported as zero
    // attempts still cost the one that produced the report.
    let mut state = SyncState::new("gmail", "conn_1");
    state.record_action(0, 0.0);
    assert_eq!(state.run_requests, 1);

    state.record_action(3, 0.01);
    assert_eq!(state.run_requests, 4);
    assert!((state.run_cost_usd - 0.01).abs() < f64::EPSILON);
}

#[test]
fn a_nonsense_cost_does_not_corrupt_the_run_total() {
    let mut state = SyncState::new("gmail", "conn_1");
    state.record_action(1, f64::NAN);
    state.record_action(1, f64::INFINITY);
    state.record_action(1, -5.0);
    assert!((state.run_cost_usd - 0.0).abs() < f64::EPSILON);
}

#[test]
fn an_unversioned_item_is_ingested_once() {
    let mut state = SyncState::new("gmail", "conn_1");
    assert!(state.needs_ingest("m1", None));

    state.mark_synced("m1", None);
    assert!(state.is_synced("m1"));
    assert!(!state.needs_ingest("m1", None));
}

#[test]
fn a_changed_item_is_ingested_again() {
    // The reason versions exist: an edited Notion page has been seen before and
    // still has to be re-ingested, while an untouched one must not be.
    let mut state = SyncState::new("notion", "conn_1");
    state.mark_synced("page-1", Some("v1"));

    assert!(!state.needs_ingest("page-1", Some("v1")));
    assert!(state.needs_ingest("page-1", Some("v2")));

    state.mark_synced("page-1", Some("v2"));
    assert!(!state.needs_ingest("page-1", Some("v2")));
}

#[tokio::test]
async fn a_connection_that_never_synced_loads_fresh_state() {
    let store = MemoryStore::default();
    let state = SyncState::load(&store, "gmail", "conn_1").await.unwrap();

    assert_eq!(state.toolkit, "gmail");
    assert_eq!(state.connection_id, "conn_1");
    assert!(state.cursor.is_none());
    assert!(state.synced_ids.is_empty());
}

#[tokio::test]
async fn state_round_trips_through_the_store() {
    let store = MemoryStore::default();
    let mut state = SyncState::new("gmail", "conn_1");
    state.cursor = Some("page-2".into());
    state.mark_synced("m1", Some("v1"));
    state.last_sync_at_ms = Some(1_772_000_000_000);
    state.save(&store).await.unwrap();

    let loaded = SyncState::load(&store, "gmail", "conn_1").await.unwrap();
    assert_eq!(loaded.cursor.as_deref(), Some("page-2"));
    assert!(loaded.is_synced("m1"));
    assert!(!loaded.needs_ingest("m1", Some("v1")));
    assert_eq!(loaded.last_sync_at_ms, Some(1_772_000_000_000));
}

#[tokio::test]
async fn run_totals_do_not_persist_into_the_next_run() {
    // They describe one run. Persisted, they would be double-counted against
    // the next one and could exhaust a budget that was never spent.
    let store = MemoryStore::default();
    let mut state = SyncState::new("gmail", "conn_1");
    state.record_action(5, 0.25);
    state.save(&store).await.unwrap();

    let loaded = SyncState::load(&store, "gmail", "conn_1").await.unwrap();
    assert_eq!(loaded.run_requests, 0);
    assert!((loaded.run_cost_usd - 0.0).abs() < f64::EPSILON);
    // The budget itself does persist — that is the point of it.
    assert_eq!(loaded.daily_budget.requests_used, 5);
}

#[tokio::test]
async fn state_is_written_under_the_pinned_namespace() {
    let store = MemoryStore::default();
    SyncState::new("gmail", "conn_1")
        .save(&store)
        .await
        .unwrap();

    let values = store.values.lock().unwrap();
    assert!(
        values.contains_key(&(STATE_NAMESPACE.to_string(), "gmail:conn_1".to_string())),
        "state must land under the pinned namespace and key"
    );
}

#[tokio::test]
async fn a_store_failure_is_reported_not_swallowed() {
    let store = MemoryStore::default();
    *store.fail.lock().unwrap() = Some("database is locked".into());

    let error = SyncState::load(&store, "gmail", "conn_1")
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Store { .. }));
}

#[tokio::test]
async fn state_that_no_longer_matches_its_shape_is_a_decode_failure() {
    // Distinguishable from a store failure, because retrying cannot fix it.
    let store = MemoryStore::default();
    store
        .set(STATE_NAMESPACE, "gmail:conn_1", &json!({ "toolkit": 42 }))
        .await
        .unwrap();

    let error = SyncState::load(&store, "gmail", "conn_1")
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Decode { .. }));
}
