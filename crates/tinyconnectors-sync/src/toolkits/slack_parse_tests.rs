//! Tests for Slack payload parsing and rendering.
//!
//! Payloads rather than call sequences: what a page *said* is a different
//! question from where the walk goes next, and it is answerable without a walk
//! at all.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use serde_json::json;

use super::{records_from, render, to_millis};
use crate::toolkits::slack::Channel;

fn channel(id: &str, name: &str) -> Channel {
    serde_json::from_value(json!({ "id": id, "name": name })).unwrap()
}

fn users() -> BTreeMap<String, String> {
    let mut users = BTreeMap::new();
    users.insert("U1".to_string(), "Ada".to_string());
    users
}

#[test]
fn references_resolve_to_something_a_person_can_read() {
    let mut users = BTreeMap::new();
    users.insert("U1".to_string(), "Ada".to_string());

    assert_eq!(render("hi <@U1>", &users), "hi @Ada");
    assert_eq!(render("see <#C7|deploys>", &users), "see #deploys");
    assert_eq!(
        render("read <https://example.com|the docs>", &users),
        "read the docs"
    );
    assert_eq!(
        render("bare <https://example.com>", &users),
        "bare https://example.com"
    );
}

#[test]
fn an_unknown_user_degrades_rather_than_dropping_the_message() {
    let users = BTreeMap::new();
    assert_eq!(render("ping <@U404>", &users), "ping @U404");
    assert_eq!(render("ping <@U404|ada>", &users), "ping @ada");
    // An unmatched bracket is text, not a broken reference.
    assert_eq!(render("2 < 3 and more", &users), "2 < 3 and more");
}

#[test]
fn a_channel_reference_without_a_label_keeps_its_id() {
    let users = BTreeMap::new();
    assert_eq!(render("see <#C7>", &users), "see #C7");
}

#[test]
fn a_slack_timestamp_becomes_milliseconds() {
    assert_eq!(to_millis("1700000000.000100"), Some(1_700_000_000_000));
    assert_eq!(to_millis("1700000000.123456"), Some(1_700_000_000_123));
    // A short fraction is padded, not read as a smaller number.
    assert_eq!(to_millis("1700000000.5"), Some(1_700_000_000_500));
    assert_eq!(to_millis("nonsense"), None);
}

#[test]
fn a_comment_and_its_file_are_two_records_rather_than_one() {
    // The comment is what someone said; the file is what they attached. Folding
    // them together would give the file the message's id, and a reader asking
    // about the file would get the remark instead.
    let message = json!({
        "ts": "1700000000.000100",
        "user": "U1",
        "text": "here is the trace",
        "files": [{ "id": "F1", "name": "trace.txt" }]
    });
    let channel = channel("C1", "eng");

    let (records, versions) = records_from(&[message], &channel, &users(), None);

    assert_eq!(records.len(), 2);
    assert_eq!(records[0].item_id, "C1:1700000000.000100");
    assert_eq!(records[0].content, "here is the trace");
    assert_eq!(records[1].item_id, "C1:1700000000.000100:F1");
    assert!(
        versions.is_empty(),
        "an unedited message reports no version, and a file never does"
    );
}

#[test]
fn an_edit_versions_the_comment_and_leaves_the_file_alone() {
    // `edited.ts` describes the message body. A file's bytes do not change: a
    // re-upload arrives as a new id, which is already a different record.
    let message = json!({
        "ts": "1700000000.000100",
        "user": "U1",
        "text": "fixed typo",
        "edited": { "ts": "1700000001.000000" },
        "files": [{ "id": "F1", "name": "trace.txt" }]
    });

    let (records, versions) = records_from(&[message], &channel("C1", "eng"), &users(), None);

    assert_eq!(records.len(), 2);
    assert_eq!(
        versions,
        vec![(
            "C1:1700000000.000100".to_string(),
            "1700000001.000000".to_string()
        )]
    );
}

#[test]
fn the_same_file_timestamp_in_two_channels_does_not_collide() {
    // `synced_ids` is one flat set for the whole connection, so an id that
    // repeats across channels would make the second channel's file look
    // already-ingested and silently drop it.
    let message = json!({
        "ts": "1700000000.000100",
        "user": "U1",
        "files": [{ "id": "F1", "name": "x.png" }]
    });

    let one = std::slice::from_ref(&message);
    let (first, _) = records_from(one, &channel("C1", "eng"), &users(), None);
    let (second, _) = records_from(one, &channel("C2", "ops"), &users(), None);

    assert_ne!(first[0].item_id, second[0].item_id);
    assert_eq!(first[0].item_id, "C1:1700000000.000100:F1");
    assert_eq!(second[0].item_id, "C2:1700000000.000100:F1");
}

#[test]
fn a_join_notice_carrying_no_files_is_still_nothing() {
    // The existing drop stays a drop: no text and no files means no record.
    let message = json!({ "ts": "1700000000.000100", "user": "U1", "text": "" });

    let (records, _) = records_from(&[message], &channel("C1", "eng"), &users(), None);

    assert!(records.is_empty());
}
