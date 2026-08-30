//! Serde representation tests for the ingestion payloads.
//!
//! The compatibility test is the important one here. `ConnectorRecord` exists to
//! be handed to memory's ingestion API without a translation step, and that is
//! only true while its wire keys match what memory accepts. If a field is
//! renamed on either side, the record still serializes fine and still
//! deserializes fine — it just silently stops carrying that field. So the key
//! set is asserted, not assumed.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{ConnectorRecord, ConnectorRecordBatch, SyncEvent, SyncStage};
use serde_json::json;

/// The wire keys memory's ingestion item accepts, in its own contract.
///
/// Kept here as a literal rather than imported: importing it would mean
/// depending on the memory contract crate, which is exactly the coupling this
/// crate exists to avoid. A literal that drifts is caught by this test; a
/// dependency that drifts is caught by nobody, because it would just compile.
const MEMORY_INGEST_KEYS: &[&str] = &[
    "item_id",
    "title",
    "content",
    "mime",
    "url",
    "updated_at_ms",
    "tags",
];

#[test]
fn a_record_serializes_exactly_the_keys_memory_ingests() {
    let record = ConnectorRecord {
        item_id: "msg-1".into(),
        title: "Lunch?".into(),
        content: "are you free at 1".into(),
        mime: Some("text/plain".into()),
        url: Some("https://mail.example.com/msg-1".into()),
        updated_at_ms: Some(1_772_000_000_000),
        tags: vec!["inbox".into()],
    };

    let value = serde_json::to_value(&record).expect("serializes");
    let mut keys: Vec<&str> = value
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();

    let mut expected = MEMORY_INGEST_KEYS.to_vec();
    expected.sort_unstable();

    assert_eq!(
        keys, expected,
        "a record must carry memory's ingestion keys and nothing else — \
         provenance belongs on the batch"
    );
}

#[test]
fn a_record_needs_only_an_id_and_content() {
    // A provider that reports no title, no URL, and no timestamp still produces
    // an ingestible record. Requiring more would drop real items.
    let record: ConnectorRecord =
        serde_json::from_value(json!({ "item_id": "x", "content": "body" })).expect("parses");

    assert_eq!(record.item_id, "x");
    assert!(record.title.is_empty());
    assert!(record.mime.is_none());
    assert!(record.url.is_none());
    assert!(record.updated_at_ms.is_none());
    assert!(record.tags.is_empty());
}

#[test]
fn a_record_without_an_id_is_rejected() {
    // The id is the dedupe key. A record without one would re-ingest as new on
    // every run and fill the user's memory with duplicates.
    assert!(serde_json::from_value::<ConnectorRecord>(json!({ "content": "body" })).is_err());
}

#[test]
fn a_batch_carries_provenance_and_paging() {
    let batch = ConnectorRecordBatch {
        source_id: "gmail:primary".into(),
        toolkit: "gmail".into(),
        connection_id: Some("conn_1".into()),
        records: vec![ConnectorRecord {
            item_id: "m1".into(),
            content: "hi".into(),
            ..ConnectorRecord::default()
        }],
        cursor: Some("page-2".into()),
        complete: false,
    };

    let value = serde_json::to_value(&batch).expect("serializes");
    assert_eq!(value["toolkit"], "gmail");
    assert_eq!(value["connection_id"], "conn_1");
    assert_eq!(value["cursor"], "page-2");
    assert_eq!(value["complete"], false);
    assert_eq!(value["records"][0]["item_id"], "m1");

    let back: ConnectorRecordBatch = serde_json::from_value(value).expect("round-trips");
    assert_eq!(back, batch);
}

#[test]
fn a_batch_omits_absent_provenance_and_paging() {
    let batch = ConnectorRecordBatch {
        source_id: "notion:workspace".into(),
        toolkit: "notion".into(),
        complete: true,
        ..ConnectorRecordBatch::default()
    };

    let value = serde_json::to_value(&batch).expect("serializes");
    assert!(value.get("connection_id").is_none());
    assert!(value.get("cursor").is_none());
    assert_eq!(value["complete"], true);
}

#[test]
fn an_empty_page_can_still_have_more_to_come() {
    // A provider may return an empty page with the cursor still set, so
    // `complete` is what ends a run — never an empty `records`.
    let batch: ConnectorRecordBatch = serde_json::from_value(json!({
        "source_id": "gmail:primary",
        "toolkit": "gmail",
        "cursor": "page-3"
    }))
    .expect("parses");

    assert!(batch.records.is_empty());
    assert!(!batch.complete);
    assert_eq!(batch.cursor.as_deref(), Some("page-3"));
}

#[test]
fn every_stage_has_a_stable_snake_case_wire_name() {
    for (stage, name) in [
        (SyncStage::Requested, "requested"),
        (SyncStage::Fetching, "fetching"),
        (SyncStage::Stored, "stored"),
        (SyncStage::Ingesting, "ingesting"),
        (SyncStage::Completed, "completed"),
        (SyncStage::Failed, "failed"),
    ] {
        assert_eq!(stage.as_str(), name);
        assert_eq!(
            serde_json::to_value(stage).expect("serializes"),
            json!(name)
        );
    }
}

#[test]
fn only_completed_and_failed_end_a_run() {
    assert!(SyncStage::Completed.is_terminal());
    assert!(SyncStage::Failed.is_terminal());
    for stage in [
        SyncStage::Requested,
        SyncStage::Fetching,
        SyncStage::Stored,
        SyncStage::Ingesting,
    ] {
        assert!(!stage.is_terminal(), "{stage:?} is not an end state");
    }
}

#[test]
fn a_sync_event_round_trips() {
    let event = SyncEvent {
        source_id: "gmail:primary".into(),
        toolkit: "gmail".into(),
        connection_id: Some("conn_1".into()),
        stage: SyncStage::Failed,
        message: Some("upstream returned 503".into()),
    };

    let value = serde_json::to_value(&event).expect("serializes");
    assert_eq!(value["stage"], "failed");

    let back: SyncEvent = serde_json::from_value(value).expect("round-trips");
    assert_eq!(back, event);
}

#[test]
fn a_sync_event_omits_an_absent_message_and_connection() {
    let event = SyncEvent {
        source_id: "notion:workspace".into(),
        toolkit: "notion".into(),
        stage: SyncStage::Completed,
        ..SyncEvent::default()
    };

    let value = serde_json::to_value(&event).expect("serializes");
    assert!(value.get("message").is_none());
    assert!(value.get("connection_id").is_none());
}
