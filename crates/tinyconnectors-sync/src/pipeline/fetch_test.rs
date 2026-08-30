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
    clean_bodies: true,
};

/// The same spec, for a toolkit whose bodies are written once and quote nothing.
const UNCLEANED: PageSpec = PageSpec {
    clean_bodies: false,
    ..SPEC
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
    assert_eq!(
        page_from(&payload, &SPEC).next_cursor.as_deref(),
        Some("p2")
    );
}

#[test]
fn an_empty_payload_yields_an_empty_final_page() {
    let page = page_from(&json!({}), &SPEC);
    assert!(page.records.is_empty());
    assert!(page.next_cursor.is_none());
}

#[test]
fn cleans_a_message_body_when_the_toolkit_asks_for_it() {
    // Otherwise the same footer arrives on every message the user has ever
    // received, and dominates any search run over the result.
    let payload = json!({
        "messages": [{
            "id": "m1",
            "body": "The real message.\n\nOn Tue, Ada wrote:\n> old thread\n\nUnsubscribe\n"
        }]
    });
    let page = page_from(&payload, &SPEC);
    assert_eq!(page.records[0].content, "The real message.");
}

#[test]
fn leaves_a_body_alone_for_a_toolkit_that_does_not_quote() {
    // An issue description is written once. Running the pass there only risks
    // cutting a line that happens to resemble a footer.
    let payload = json!({
        "messages": [{ "id": "i1", "body": "Steps:\n> run the thing\n> it fails\n> every time" }]
    });
    let page = page_from(&payload, &UNCLEANED);
    assert!(page.records[0].content.contains("every time"));
}

#[test]
fn caps_a_body_that_would_outweigh_a_hundred_others() {
    let huge = "word ".repeat(20_000);
    let payload = json!({ "messages": [{ "id": "m1", "body": huge }] });
    let page = page_from(&payload, &UNCLEANED);

    assert!(page.records[0].content.chars().count() < 21_000);
    assert!(page.records[0].content.ends_with("[truncated]"));
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
