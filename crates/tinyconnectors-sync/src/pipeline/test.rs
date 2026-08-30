//! Unit tests for the sync loop and its helpers.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;
use tinyconnectors_bus::{ConnectorRecord, SyncStage};

use super::{
    MIN_PAGE_SIZE, ProviderPage, first_array, is_payload_too_large, next_page_token, pick_str,
    run_sync, shrink_page_size,
};
use crate::provider::{
    ActionRunner, ConnectorProvider, ProviderContext, ProviderUserProfile, SyncLimits, SyncReason,
};
use crate::state::{SyncState, SyncStateStore};
use crate::{Error, Result};

// ── json helpers ────────────────────────────────────────────────────

#[test]
fn picks_the_first_non_empty_scalar() {
    let value = json!({ "data": { "id": "", "messageId": "m1" } });
    assert_eq!(
        pick_str(&value, &["data.id", "data.messageId"]).as_deref(),
        Some("m1")
    );
}

#[test]
fn coerces_a_number_to_its_string_form() {
    // The same provider field arrives as `"123"` from one endpoint and `123`
    // from another, and a caller building a record id cannot care which.
    let value = json!({ "id": 123 });
    assert_eq!(pick_str(&value, &["id"]).as_deref(), Some("123"));
}

#[test]
fn indexes_into_an_array_by_a_numeric_segment() {
    let value = json!({ "messages": [{ "id": "m1" }, { "id": "m2" }] });
    assert_eq!(pick_str(&value, &["messages.1.id"]).as_deref(), Some("m2"));
}

#[test]
fn finds_the_first_array_that_exists() {
    let value = json!({ "data": { "messages": [1, 2] } });
    assert_eq!(first_array(&value, &["/items", "/data/messages"]).len(), 2);
    assert!(first_array(&value, &["/nope"]).is_empty());
}

#[test]
fn reads_a_page_token_from_any_envelope_shape() {
    for envelope in [
        json!({ "nextPageToken": "t" }),
        json!({ "data": { "nextPageToken": "t" } }),
        json!({ "data": { "data": { "nextPageToken": "t" } } }),
        json!({ "next_page_token": "t" }),
    ] {
        assert_eq!(next_page_token(&envelope).as_deref(), Some("t"), "{envelope}");
    }
}

#[test]
fn treats_an_empty_page_token_as_no_more_pages() {
    // Several providers say "done" with an empty string. Treating it as a
    // cursor asks forever for a page that does not exist.
    assert!(next_page_token(&json!({ "nextPageToken": "" })).is_none());
    assert!(next_page_token(&json!({ "nextPageToken": "   " })).is_none());
    assert!(next_page_token(&json!({})).is_none());
}

// ── page size ───────────────────────────────────────────────────────

#[test]
fn recognizes_a_page_refused_for_its_size() {
    for message in [
        "Upstream_PayloadTooLarge",
        "payload_too_large",
        "HTTP 413 returned",
        "the response was too large",
    ] {
        assert!(is_payload_too_large(Some(message)), "{message}");
    }
}

#[test]
fn does_not_mistake_digits_for_a_413_status() {
    // An unanchored match also hits a message id or an amount, and each false
    // match costs a shrink-and-retry before the real error surfaces.
    for message in ["message id 84130", "amount 1413", "code 4131"] {
        assert!(!is_payload_too_large(Some(message)), "{message}");
    }
    assert!(!is_payload_too_large(None));
    assert!(!is_payload_too_large(Some("insufficient scope")));
}

#[test]
fn halves_a_page_until_it_cannot_help() {
    assert_eq!(shrink_page_size(100), Some(50));
    assert_eq!(shrink_page_size(3), Some(1));
    assert_eq!(shrink_page_size(2), Some(1));
    // Past one item, a too-large response is about that item, not the batch.
    assert_eq!(shrink_page_size(MIN_PAGE_SIZE), None);
    assert_eq!(shrink_page_size(0), None);
}

// ── the loop ────────────────────────────────────────────────────────

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

#[derive(Debug)]
struct NoActions;

#[async_trait]
impl ActionRunner for NoActions {
    async fn run(&self, _: &str, _: serde_json::Value, _: &str) -> Result<serde_json::Value> {
        Ok(json!({}))
    }
}

/// A provider that replays scripted pages.
#[derive(Debug)]
struct ScriptedProvider {
    pages: Mutex<Vec<Result<ProviderPage>>>,
    requested_cursors: Mutex<Vec<Option<String>>>,
}

impl ScriptedProvider {
    fn new(pages: Vec<Result<ProviderPage>>) -> Self {
        Self {
            pages: Mutex::new(pages),
            requested_cursors: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl ConnectorProvider for ScriptedProvider {
    fn toolkit_slug(&self) -> &'static str {
        "gmail"
    }
    fn description(&self) -> &'static str {
        "a scripted provider."
    }
    async fn fetch_user_profile(&self, _: &ProviderContext) -> Result<ProviderUserProfile> {
        Ok(ProviderUserProfile::default())
    }
    async fn fetch_page(
        &self,
        _context: &ProviderContext,
        cursor: Option<&str>,
    ) -> Result<ProviderPage> {
        self.requested_cursors
            .lock()
            .unwrap()
            .push(cursor.map(str::to_string));
        let mut pages = self.pages.lock().unwrap();
        if pages.is_empty() {
            return Ok(ProviderPage::default());
        }
        match pages.remove(0) {
            Ok(page) => Ok(page),
            Err(error) => Err(error),
        }
    }
}

fn record(id: &str) -> ConnectorRecord {
    ConnectorRecord {
        item_id: id.to_string(),
        content: format!("body of {id}"),
        ..ConnectorRecord::default()
    }
}

fn page(ids: &[&str], next: Option<&str>) -> Result<ProviderPage> {
    Ok(ProviderPage {
        records: ids.iter().map(|id| record(id)).collect(),
        versions: Vec::new(),
        next_cursor: next.map(str::to_string),
    })
}

fn context(store: Arc<MemoryStore>, max_items: usize) -> ProviderContext {
    ProviderContext {
        toolkit: "gmail".into(),
        connection_id: "conn_1".into(),
        source_id: "gmail:primary".into(),
        limits: SyncLimits {
            max_items,
            depth_days: Some(90),
        },
        actions: Arc::new(NoActions),
        state: store,
    }
}

#[tokio::test]
async fn pages_until_the_provider_runs_out() {
    let provider = ScriptedProvider::new(vec![
        page(&["m1", "m2"], Some("p2")),
        page(&["m3"], None),
    ]);
    let store = Arc::new(MemoryStore::default());

    let outcome = run_sync(&provider, &context(store, 100), SyncReason::Manual)
        .await
        .unwrap();

    assert_eq!(outcome.stage, SyncStage::Completed);
    assert!(outcome.batch.complete, "the provider had no more to give");
    assert_eq!(outcome.batch.records.len(), 3);
    assert_eq!(outcome.pages_read, 2);
    assert_eq!(
        *provider.requested_cursors.lock().unwrap(),
        vec![None, Some("p2".to_string())]
    );
}

#[tokio::test]
async fn stops_at_the_item_limit_and_leaves_the_run_incomplete() {
    // Stopped by a limit means there is more to read: the batch must not claim
    // completion, or the host stops asking.
    let provider = ScriptedProvider::new(vec![page(&["m1", "m2", "m3"], Some("p2"))]);
    let store = Arc::new(MemoryStore::default());

    let outcome = run_sync(&provider, &context(store, 2), SyncReason::Scheduled)
        .await
        .unwrap();

    assert_eq!(outcome.batch.records.len(), 2);
    assert!(!outcome.batch.complete);
    assert_eq!(outcome.batch.cursor.as_deref(), Some("p2"));
}

#[tokio::test]
async fn resumes_from_the_stored_cursor() {
    let store = Arc::new(MemoryStore::default());
    let mut state = SyncState::new("gmail", "conn_1");
    state.advance_cursor("p5");
    state.save(store.as_ref()).await.unwrap();

    let provider = ScriptedProvider::new(vec![page(&["m9"], None)]);
    run_sync(&provider, &context(store, 100), SyncReason::Scheduled)
        .await
        .unwrap();

    assert_eq!(
        *provider.requested_cursors.lock().unwrap(),
        vec![Some("p5".to_string())],
        "a run must resume rather than re-read the whole mailbox"
    );
}

#[tokio::test]
async fn skips_records_already_ingested() {
    let store = Arc::new(MemoryStore::default());
    let mut state = SyncState::new("gmail", "conn_1");
    state.mark_synced("m1", None);
    state.save(store.as_ref()).await.unwrap();

    let provider = ScriptedProvider::new(vec![page(&["m1", "m2"], None)]);
    let outcome = run_sync(&provider, &context(store, 100), SyncReason::Scheduled)
        .await
        .unwrap();

    assert_eq!(outcome.batch.records.len(), 1);
    assert_eq!(outcome.batch.records[0].item_id, "m2");
    assert_eq!(outcome.records_skipped, 1);
}

#[tokio::test]
async fn re_ingests_a_record_whose_version_changed() {
    // An edited page has been seen before and still has to be re-ingested.
    let store = Arc::new(MemoryStore::default());
    let mut state = SyncState::new("gmail", "conn_1");
    state.mark_synced("p1", Some("v1"));
    state.save(store.as_ref()).await.unwrap();

    let provider = ScriptedProvider::new(vec![Ok(ProviderPage {
        records: vec![record("p1")],
        versions: vec![("p1".to_string(), "v2".to_string())],
        next_cursor: None,
    })]);
    let outcome = run_sync(&provider, &context(store, 100), SyncReason::Scheduled)
        .await
        .unwrap();

    assert_eq!(outcome.batch.records.len(), 1);
    assert_eq!(outcome.records_skipped, 0);
}

#[tokio::test]
async fn keeps_what_it_read_when_a_later_page_fails() {
    // A connection failing on its fifth page must still ingest its first four.
    let provider = ScriptedProvider::new(vec![
        page(&["m1"], Some("p2")),
        Err(Error::Action {
            action: "GMAIL_FETCH_EMAILS".into(),
            message: "upstream 503".into(),
        }),
    ]);
    let store = Arc::new(MemoryStore::default());

    let outcome = run_sync(&provider, &context(store.clone(), 100), SyncReason::Manual)
        .await
        .unwrap();

    assert_eq!(outcome.stage, SyncStage::Failed);
    assert_eq!(outcome.batch.records.len(), 1, "the first page survives");
    assert!(!outcome.batch.complete);
    assert!(outcome.message.unwrap().contains("upstream 503"));

    // And the cursor is saved, so the next attempt does not re-read page one.
    let state = SyncState::load(store.as_ref(), "gmail", "conn_1")
        .await
        .unwrap();
    assert_eq!(state.cursor.as_deref(), Some("p2"));
}

#[tokio::test]
async fn does_not_start_when_the_day_s_budget_is_spent() {
    let store = Arc::new(MemoryStore::default());
    let mut state = SyncState::new("gmail", "conn_1");
    state.record_action(state.daily_budget.limit, 0.0);
    state.save(store.as_ref()).await.unwrap();

    let provider = ScriptedProvider::new(vec![page(&["m1"], None)]);
    let outcome = run_sync(&provider, &context(store, 100), SyncReason::Scheduled)
        .await
        .unwrap();

    // Not a failure: the budget did its job, and a red status on a connection
    // working as configured is worse than no status.
    assert_eq!(outcome.stage, SyncStage::Completed);
    assert_eq!(outcome.pages_read, 0);
    assert!(outcome.message.unwrap().contains("budget"));
    assert!(provider.requested_cursors.lock().unwrap().is_empty());
}

#[tokio::test]
async fn records_the_batch_provenance() {
    let provider = ScriptedProvider::new(vec![page(&["m1"], None)]);
    let store = Arc::new(MemoryStore::default());

    let outcome = run_sync(&provider, &context(store, 100), SyncReason::InitialConnect)
        .await
        .unwrap();

    assert_eq!(outcome.batch.source_id, "gmail:primary");
    assert_eq!(outcome.batch.toolkit, "gmail");
    assert_eq!(outcome.batch.connection_id.as_deref(), Some("conn_1"));
    assert_eq!(outcome.reason, SyncReason::InitialConnect);
}

#[tokio::test]
async fn a_provider_with_nothing_to_read_completes_immediately() {
    let provider = ScriptedProvider::new(Vec::new());
    let store = Arc::new(MemoryStore::default());

    let outcome = run_sync(&provider, &context(store, 100), SyncReason::Scheduled)
        .await
        .unwrap();

    assert!(outcome.batch.records.is_empty());
    assert!(outcome.batch.complete);
    assert_eq!(outcome.stage, SyncStage::Completed);
}
