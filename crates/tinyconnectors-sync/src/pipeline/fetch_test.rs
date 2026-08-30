//! Unit tests for the declarative page read.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;

use super::{PageSpec, page_from};

const SPEC: PageSpec = PageSpec {
    action: "TEST_FETCH",
    item_pointers: &["/data/messages", "/messages"],
    id_paths: &["id", "messageId"],
    title_paths: &["subject", "title"],
    content_paths: &["body", "text"],
    url_paths: &["url", "webLink"],
    version_paths: &["version", "etag"],
    page_size_arg: "max_results",
    cursor_arg: "page_token",
};

#[test]
fn reads_items_from_the_first_pointer_that_matches() {
    let payload = json!({
        "data": { "messages": [{ "id": "m1", "subject": "Hi", "body": "there" }] }
    });
    let page = page_from(&payload, &SPEC);

    assert_eq!(page.records.len(), 1);
    assert_eq!(page.records[0].item_id, "m1");
    assert_eq!(page.records[0].title, "Hi");
    assert_eq!(page.records[0].content, "there");
}

#[test]
fn falls_back_through_the_id_and_title_paths() {
    let payload = json!({ "messages": [{ "messageId": "m2", "title": "Second" }] });
    let page = page_from(&payload, &SPEC);
    assert_eq!(page.records[0].item_id, "m2");
    assert_eq!(page.records[0].title, "Second");
}

#[test]
fn drops_an_item_with_no_derivable_id() {
    // The id is the dedupe key. A record without one re-ingests as new on every
    // run, filling the user's memory with copies of one thing.
    let payload = json!({ "messages": [{ "subject": "no id here" }, { "id": "m1" }] });
    let page = page_from(&payload, &SPEC);
    assert_eq!(page.records.len(), 1);
    assert_eq!(page.records[0].item_id, "m1");
}

#[test]
fn keeps_the_whole_item_when_no_body_field_matches() {
    // An unrecognized body shape still carries something an agent can read,
    // which beats ingesting an empty record.
    let payload = json!({ "messages": [{ "id": "m1", "snippetText": "hello" }] });
    let page = page_from(&payload, &SPEC);
    assert!(page.records[0].content.contains("snippetText"));
    assert!(!page.records[0].content.is_empty());
}

#[test]
fn collects_a_version_only_when_the_item_reports_one() {
    let payload = json!({
        "messages": [
            { "id": "m1", "version": "v3" },
            { "id": "m2" }
        ]
    });
    let page = page_from(&payload, &SPEC);
    assert_eq!(page.versions, vec![("m1".to_string(), "v3".to_string())]);
}

#[test]
fn reads_the_next_cursor_from_the_envelope() {
    let payload = json!({ "messages": [], "nextPageToken": "p2" });
    assert_eq!(page_from(&payload, &SPEC).next_cursor.as_deref(), Some("p2"));
}

#[test]
fn an_empty_payload_yields_an_empty_final_page() {
    let page = page_from(&json!({}), &SPEC);
    assert!(page.records.is_empty());
    assert!(page.next_cursor.is_none());
}

#[test]
fn carries_a_link_back_to_the_item_when_there_is_one() {
    let payload = json!({
        "messages": [{ "id": "m1", "webLink": "https://mail.example.com/m1" }]
    });
    let page = page_from(&payload, &SPEC);
    assert_eq!(
        page.records[0].url.as_deref(),
        Some("https://mail.example.com/m1")
    );
}
