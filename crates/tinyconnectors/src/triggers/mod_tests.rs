//! Unit tests for the trigger archive.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use tinystoragedrivers_core::{
    MemoryStorage, Scope, StorageBackend, StorageError, StreamEntry, StreamStore,
};

use super::{DEFAULT_HISTORY_LIMIT, TriggerArchive};

fn streams() -> Arc<dyn StreamStore> {
    MemoryStorage::new()
        .for_scope(&Scope::new("alice").unwrap())
        .unwrap()
        .streams()
        .clone()
}

fn archive() -> (Arc<dyn StreamStore>, TriggerArchive) {
    let streams = streams();
    (streams.clone(), TriggerArchive::new(streams))
}

fn ids(history: &crate::ComposioTriggerHistoryResult) -> Vec<&str> {
    history
        .entries
        .iter()
        .map(|entry| entry.metadata_id.as_str())
        .collect()
}

#[tokio::test]
async fn records_a_delivery_and_reads_it_back() {
    let (_, archive) = archive();
    let entry = archive
        .record(
            "gmail",
            "GMAIL_NEW_GMAIL_MESSAGE",
            "evt-1",
            "uuid-1",
            &json!({ "subject": "hi" }),
        )
        .await
        .expect("records");

    assert_eq!(entry.toolkit, "gmail");
    assert!(entry.received_at_ms > 0);

    let history = archive.list_recent(None).await.expect("reads");
    assert_eq!(history.entries.len(), 1);
    assert_eq!(history.entries[0].metadata_id, "evt-1");
    assert_eq!(history.entries[0].payload["subject"], "hi");
    assert_eq!(history.current_day_file, archive.current_day_file());
    assert_eq!(history.archive_dir, "connectors.triggers");
}

#[tokio::test]
async fn returns_the_newest_deliveries_first() {
    let (_, archive) = archive();
    for index in 0..5 {
        archive
            .record("gmail", "T", &format!("evt-{index}"), "u", &json!({}))
            .await
            .expect("records");
    }

    let history = archive.list_recent(None).await.expect("reads");
    assert_eq!(ids(&history), ["evt-4", "evt-3", "evt-2", "evt-1", "evt-0"]);
}

#[tokio::test]
async fn honours_the_requested_limit() {
    let (_, archive) = archive();
    for index in 0..10 {
        archive
            .record("gmail", "T", &format!("evt-{index}"), "u", &json!({}))
            .await
            .expect("records");
    }

    let history = archive.list_recent(Some(3)).await.expect("reads");
    assert_eq!(ids(&history), ["evt-9", "evt-8", "evt-7"]);
}

#[tokio::test]
async fn a_zero_limit_still_returns_something() {
    // Asking for nothing is a caller mistake, not a request to be answered
    // literally with an empty list that looks like "no triggers ever fired".
    let (_, archive) = archive();
    archive
        .record("gmail", "T", "evt-0", "u", &json!({}))
        .await
        .expect("records");

    let history = archive.list_recent(Some(0)).await.expect("reads");
    assert_eq!(history.entries.len(), 1);
}

#[tokio::test]
async fn an_empty_archive_reads_as_empty_rather_than_failing() {
    let (_, archive) = archive();
    let history = archive.list_recent(None).await.expect("reads");
    assert!(history.entries.is_empty());
}

#[tokio::test]
async fn a_corrupt_record_does_not_hide_the_rest() {
    let (streams, archive) = archive();
    archive
        .record("gmail", "T", "evt-0", "u", &json!({}))
        .await
        .expect("records");
    streams
        .append(
            &archive.current_day_file(),
            json!({ "received_at_ms": "oops" }),
        )
        .await
        .expect("appends junk");
    archive
        .record("gmail", "T", "evt-1", "u", &json!({}))
        .await
        .expect("records");

    let history = archive.list_recent(None).await.expect("reads");
    assert_eq!(ids(&history), ["evt-1", "evt-0"]);
}

#[tokio::test]
async fn a_corrupt_newest_record_is_backfilled_from_older_ones() {
    let (streams, archive) = archive();
    for id in ["evt-0", "evt-1", "evt-2"] {
        archive
            .record("gmail", "T", id, "u", &json!({}))
            .await
            .expect("records");
    }
    streams
        .append(
            &archive.current_day_file(),
            json!({ "received_at_ms": "oops" }),
        )
        .await
        .expect("appends junk");

    let history = archive.list_recent(Some(2)).await.expect("reads");
    assert_eq!(ids(&history), ["evt-2", "evt-1"]);
}

#[tokio::test]
async fn ignores_streams_that_are_not_archive_days() {
    let (streams, archive) = archive();
    archive
        .record("gmail", "T", "evt-0", "u", &json!({}))
        .await
        .expect("records");
    streams
        .append("other/2000-01-01", json!({ "junk": true }))
        .await
        .expect("appends");

    let history = archive.list_recent(None).await.expect("reads");
    assert_eq!(history.entries.len(), 1);
}

#[tokio::test]
async fn the_default_limit_is_a_window_not_everything() {
    // The archive grows without bound, so the default has to be a window, or a
    // history read eventually returns a user's entire trigger history.
    let (_, archive) = archive();
    for index in 0..(DEFAULT_HISTORY_LIMIT + 10) {
        archive
            .record("gmail", "T", &format!("evt-{index}"), "u", &json!({}))
            .await
            .expect("records");
    }

    let history = archive.list_recent(None).await.expect("reads");
    assert_eq!(history.entries.len(), DEFAULT_HISTORY_LIMIT);
    assert_eq!(
        history.entries[0].metadata_id,
        format!("evt-{}", DEFAULT_HISTORY_LIMIT + 9)
    );
}

#[tokio::test]
async fn writes_land_in_the_current_day_stream() {
    let (streams, archive) = archive();
    archive
        .record("gmail", "T", "evt-0", "u", &json!({}))
        .await
        .expect("records");

    let name = archive.current_day_file();
    assert!(name.starts_with("connectors.triggers/"));
    assert_eq!(
        streams.len(&name).await.unwrap(),
        1,
        "one record per delivery"
    );
}

#[tokio::test]
async fn reads_across_more_than_one_day() {
    // Newest day first, then newest within the day: the ordering that makes
    // "the last twenty" mean what a user expects.
    let (streams, archive) = archive();
    archive
        .record("gmail", "T", "today-0", "u", &json!({}))
        .await
        .expect("records");
    streams
        .append(
            "connectors.triggers/2000-01-01",
            json!({
                "received_at_ms": 1, "toolkit": "gmail", "trigger": "T",
                "metadata_id": "old-0", "metadata_uuid": "u", "payload": {}
            }),
        )
        .await
        .expect("appends");

    let history = archive.list_recent(None).await.expect("reads");
    assert_eq!(ids(&history), ["today-0", "old-0"]);
    let limited = archive.list_recent(Some(1)).await.expect("reads");
    assert_eq!(ids(&limited), ["today-0"]);
}

#[tokio::test]
async fn concurrent_deliveries_are_all_kept() {
    let (_, archive) = archive();
    let mut tasks = Vec::new();
    for index in 0..20 {
        let archive = archive.clone();
        tasks.push(tokio::spawn(async move {
            archive
                .record("gmail", "T", &format!("evt-{index}"), "u", &json!({}))
                .await
                .unwrap();
        }));
    }
    for task in tasks {
        task.await.unwrap();
    }
    let history = archive.list_recent(Some(100)).await.expect("reads");
    assert_eq!(history.entries.len(), 20);
}

#[derive(Debug)]
struct Broken;

#[async_trait]
impl StreamStore for Broken {
    async fn append(&self, _: &str, _: Value) -> tinystoragedrivers_core::Result<u64> {
        Err(StorageError::invalid_input("down"))
    }
    async fn append_batch(&self, _: &str, _: Vec<Value>) -> tinystoragedrivers_core::Result<u64> {
        Err(StorageError::invalid_input("down"))
    }
    async fn read_window(
        &self,
        _: &str,
        _: u64,
        _: usize,
    ) -> tinystoragedrivers_core::Result<Vec<StreamEntry>> {
        Err(StorageError::invalid_input("down"))
    }
    async fn len(&self, _: &str) -> tinystoragedrivers_core::Result<u64> {
        Err(StorageError::invalid_input("down"))
    }
    async fn truncate_before(&self, _: &str, _: u64) -> tinystoragedrivers_core::Result<u64> {
        Err(StorageError::invalid_input("down"))
    }
    async fn delete_stream(&self, _: &str) -> tinystoragedrivers_core::Result<bool> {
        Err(StorageError::invalid_input("down"))
    }
    async fn streams(&self, _: &str) -> tinystoragedrivers_core::Result<Vec<String>> {
        Err(StorageError::invalid_input("down"))
    }
}

#[tokio::test]
async fn storage_failures_are_reported_not_swallowed() {
    // A delivery that cannot be archived is still delivered, but the failure
    // must not be silent.
    let archive = TriggerArchive::new(Arc::new(Broken));
    let error = archive
        .record("gmail", "T", "evt-0", "u", &json!({}))
        .await
        .expect_err("must report");
    assert!(matches!(error, crate::Error::Archive { .. }));
    assert!(archive.list_recent(None).await.is_err());
}
