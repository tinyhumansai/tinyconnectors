//! Tests for Slack file shares becoming records.
//!
//! Payloads rather than call sequences, for the same reason the parsing tests
//! beside these are: what a page *carried* is answerable without a walk.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use serde_json::json;

use super::file_records_from;
use crate::toolkits::slack::Channel;

fn channel() -> Channel {
    serde_json::from_value(json!({ "id": "C1", "name": "eng" })).unwrap()
}

fn users() -> BTreeMap<String, String> {
    let mut users = BTreeMap::new();
    users.insert("U1".to_string(), "Ada".to_string());
    users
}

#[test]
fn a_file_share_with_no_message_text_still_becomes_a_record() {
    let message = json!({
        "ts": "1700000000.000100",
        "user": "U1",
        "text": "",
        "files": [{
            "id": "F1",
            "name": "screenshot.png",
            "mimetype": "image/png",
            "permalink": "https://slack.com/files/F1",
            "created": 1_700_000_500
        }]
    });

    let records = file_records_from(&message, &channel(), &users(), false);

    assert_eq!(records.len(), 1);
    assert_eq!(records[0].item_id, "C1:1700000000.000100:F1");
    assert_eq!(records[0].title, "#eng — Ada shared screenshot.png");
    assert!(records[0].content.contains("screenshot.png"));
    assert!(records[0].content.contains("image/png"));
    assert_eq!(records[0].mime.as_deref(), Some("text/plain"));
    assert_eq!(
        records[0].url.as_deref(),
        Some("https://slack.com/files/F1")
    );
    // The file's own clock, not the message's.
    assert_eq!(records[0].updated_at_ms, Some(1_700_000_500_000));
}

#[test]
fn every_file_on_one_message_gets_its_own_id() {
    let message = json!({
        "ts": "1700000000.000100",
        "user": "U1",
        "files": [
            { "id": "F1", "name": "one.png" },
            { "id": "F2", "name": "two.png" }
        ]
    });

    let records = file_records_from(&message, &channel(), &users(), false);

    assert_eq!(records.len(), 2);
    assert_ne!(records[0].item_id, records[1].item_id);
    assert_eq!(records[0].item_id, "C1:1700000000.000100:F1");
    assert_eq!(records[1].item_id, "C1:1700000000.000100:F2");
}

#[test]
fn a_file_without_an_id_or_anything_to_call_it_is_skipped() {
    // The id is the dedupe key, so a file lacking one would re-ingest as new on
    // every run. The second entry has neither a name nor a title, which is what
    // a tombstone for a deleted file looks like.
    let message = json!({
        "ts": "1700000000.000100",
        "user": "U1",
        "files": [
            { "name": "no-id.png" },
            { "id": "F2" },
            { "id": "F3", "name": "kept.png" }
        ]
    });

    let records = file_records_from(&message, &channel(), &users(), false);

    assert_eq!(records.len(), 1);
    assert_eq!(records[0].item_id, "C1:1700000000.000100:F3");
}

#[test]
fn a_message_without_files_or_a_timestamp_yields_nothing() {
    let plain = json!({ "ts": "1700000000.000100", "user": "U1", "text": "hello" });
    assert!(file_records_from(&plain, &channel(), &users(), false).is_empty());

    let undated = json!({ "user": "U1", "files": [{ "id": "F1", "name": "x.png" }] });
    assert!(file_records_from(&undated, &channel(), &users(), false).is_empty());

    // `files` present but not an array — Composio envelopes vary, and a shape
    // this code cannot walk must not panic.
    let malformed = json!({ "ts": "1700000000.000100", "files": "nope" });
    assert!(file_records_from(&malformed, &channel(), &users(), false).is_empty());
}

#[test]
fn a_snippets_preview_is_kept_as_body_text() {
    let message = json!({
        "ts": "1700000000.000100",
        "user": "U1",
        "files": [{
            "id": "F1",
            "name": "deploy.log",
            "title": "Deploy output",
            "mimetype": "text/plain",
            "preview": "line one\nline two"
        }]
    });

    let records = file_records_from(&message, &channel(), &users(), false);

    assert_eq!(records.len(), 1);
    let content = &records[0].content;
    assert!(content.contains("deploy.log"), "{content}");
    // The human title is kept because it says something the filename does not.
    assert!(content.contains("Deploy output"), "{content}");
    assert!(content.contains("line one"), "{content}");
}

#[test]
fn a_title_matching_the_filename_is_not_repeated() {
    let message = json!({
        "ts": "1700000000.000100",
        "user": "U1",
        "files": [{ "id": "F1", "name": "same.png", "title": "same.png" }]
    });

    let records = file_records_from(&message, &channel(), &users(), false);

    assert_eq!(records[0].content, "same.png");
}

#[test]
fn a_file_in_a_thread_reply_is_marked_as_one() {
    let message = json!({
        "ts": "1700000000.000100",
        "user": "U1",
        "files": [{ "id": "F1", "name": "patch.diff" }]
    });

    let records = file_records_from(&message, &channel(), &users(), true);

    assert_eq!(records[0].title, "#eng — Ada shared patch.diff (reply)");
}

#[test]
fn an_unresolved_uploader_keeps_its_id_rather_than_collapsing() {
    let message = json!({
        "ts": "1700000000.000100",
        "user": "U9",
        "files": [{ "id": "F1", "name": "x.png" }]
    });

    let records = file_records_from(&message, &channel(), &users(), false);

    assert_eq!(records[0].title, "#eng — U9 shared x.png");
}

#[test]
fn a_file_without_its_own_clock_falls_back_to_the_message() {
    let message = json!({
        "ts": "1700000000.000100",
        "user": "U1",
        "files": [{ "id": "F1", "name": "x.png" }]
    });

    let records = file_records_from(&message, &channel(), &users(), false);

    assert_eq!(records[0].updated_at_ms, Some(1_700_000_000_000));
    assert_eq!(
        records[0].url, None,
        "no permalink means no url, not a guess"
    );
}

#[test]
fn a_file_is_credited_to_its_uploader_not_the_poster() {
    // Re-sharing someone else's upload, or a bot posting a file a person made,
    // puts two different people on one message.
    let mut directory = users();
    directory.insert("U2".to_string(), "Grace".to_string());
    let message = json!({
        "ts": "1700000000.000100",
        "user": "U1",
        "text": "look at this",
        "files": [{ "id": "F1", "name": "grace.png", "user": "U2" }]
    });

    let records = file_records_from(&message, &channel(), &directory, false);

    assert_eq!(records[0].title, "#eng — Grace shared grace.png");
}

#[test]
fn a_file_naming_no_uploader_falls_back_to_the_poster() {
    let message = json!({
        "ts": "1700000000.000100",
        "user": "U1",
        "files": [{ "id": "F1", "name": "x.png" }]
    });

    let records = file_records_from(&message, &channel(), &users(), false);

    assert_eq!(records[0].title, "#eng — Ada shared x.png");
}

#[test]
fn a_file_with_only_a_title_is_kept_rather_than_dropped() {
    // `title` is enough to recognise a file by, and dropping it would be the
    // same silent loss this module exists to undo. Only a payload with neither
    // a name nor a title is a tombstone.
    let message = json!({
        "ts": "1700000000.000100",
        "user": "U1",
        "files": [{ "id": "F1", "title": "Deploy notes" }]
    });

    let records = file_records_from(&message, &channel(), &users(), false);

    assert_eq!(records.len(), 1);
    assert_eq!(records[0].title, "#eng — Ada shared Deploy notes");
}

#[test]
fn an_edited_file_reports_when_it_was_edited_not_when_it_was_made() {
    // `updated_at_ms` is the upstream last-modified time. A Slack Post edited
    // after upload reports both, and `created` would understate it.
    let message = json!({
        "ts": "1700000000.000100",
        "user": "U1",
        "files": [{
            "id": "F1",
            "name": "notes.post",
            "created": 1_700_000_100,
            "updated": 1_700_009_000
        }]
    });

    let records = file_records_from(&message, &channel(), &users(), false);

    assert_eq!(records[0].updated_at_ms, Some(1_700_009_000_000));
}
