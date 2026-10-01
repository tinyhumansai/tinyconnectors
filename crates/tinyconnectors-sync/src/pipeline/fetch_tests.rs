//! Unit tests for the declarative page read.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;

use super::{DepthWindow, PageSpec, Paging, item_count, next_page_number, page_from, page_number};

const SPEC: PageSpec = PageSpec {
    action: "TEST_FETCH",
    item_pointers: &["/data/messages", "/messages"],
    id_paths: &["id", "messageId"],
    title_paths: &["subject", "title"],
    content_paths: &["body", "text"],
    url_paths: &["url", "webLink"],
    version_paths: &["version", "etag"],
    fixed_arguments: &[],
    page_size_arg: "max_results",
    cursor_arg: "page_token",
    paging: Paging::Token {
        next: &["/data/nextPageToken", "/nextPageToken"],
    },

    depth_window: None,
    clean_bodies: true,
};

/// The same spec, for a toolkit whose bodies are written once and quote nothing.
const UNCLEANED: PageSpec = PageSpec {
    clean_bodies: false,
    ..SPEC
};

/// The same spec, paging a GraphQL connection.
const CONNECTION: PageSpec = PageSpec {
    paging: Paging::PageInfo {
        page_info: &["/data/pageInfo", "/pageInfo"],
    },
    ..SPEC
};

/// The same spec, paging by number.
const NUMBERED: PageSpec = PageSpec {
    paging: Paging::Numbered {
        first: 1,
        reachable: None,
        last_page: &[],
    },
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
fn follows_a_connection_only_while_it_has_a_next_page() {
    let more = json!({ "data": { "pageInfo": { "hasNextPage": true, "endCursor": "e1" } } });
    assert_eq!(
        page_from(&more, &CONNECTION).next_cursor.as_deref(),
        Some("e1")
    );
    let done = json!({ "pageInfo": { "hasNextPage": false, "endCursor": "e1" } });
    assert!(page_from(&done, &CONNECTION).next_cursor.is_none());
}

#[test]
fn a_numbered_page_leaves_its_successor_to_the_read() {
    // Only the read knows which page it asked for, so the payload alone names
    // no next page, whatever token it happens to carry.
    let payload = json!({ "messages": [{ "id": "m1" }], "nextPageToken": "p2" });
    assert!(page_from(&payload, &NUMBERED).next_cursor.is_none());
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

#[test]
fn a_numbered_read_starts_at_the_first_page() {
    assert_eq!(page_number(None, 1), 1);
    assert_eq!(page_number(Some(" 4 "), 1), 4);
    assert_eq!(page_number(None, 0), 0);
    assert_eq!(page_number(Some("0"), 0), 0);
    // A position this build cannot read starts the walk over rather than
    // stalling it: the seen-set turns the re-read into skips.
    for unreadable in ["", "0", "-2", "page-2"] {
        assert_eq!(page_number(Some(unreadable), 1), 1, "{unreadable:?}");
    }
}

#[test]
fn a_full_page_is_followed_by_the_next() {
    assert_eq!(next_page_number(1, 1, Some(1_000), 50, 50, None), Some(2));
    // A provider that hands back more than was asked for has not run out.
    assert_eq!(next_page_number(1, 1, Some(1_000), 60, 50, None), Some(2));
    // With no cap on what the provider serves, the walk goes on while pages do.
    assert_eq!(next_page_number(500, 0, None, 100, 100, None), Some(501));
}

#[test]
fn a_short_page_is_the_last() {
    assert_eq!(next_page_number(3, 1, Some(1_000), 49, 50, None), None);
}

#[test]
fn an_empty_page_is_the_last_whatever_its_flag_says() {
    assert_eq!(next_page_number(1, 1, None, 0, 50, None), None);
    assert_eq!(next_page_number(1, 0, None, 0, 50, Some(false)), None);
}

#[test]
fn a_last_page_flag_is_believed_over_the_page_size() {
    // `ClickUp` serves its own page size, so a page shorter than the one asked
    // for is not the last unless it says so, and a full one can be.
    assert_eq!(next_page_number(2, 0, None, 10, 50, Some(false)), Some(3));
    assert_eq!(next_page_number(2, 0, None, 100, 50, Some(true)), None);
}

#[test]
fn a_walk_stops_where_the_provider_stops_serving() {
    // GitHub's search answers a page past its thousandth result with an error.
    assert_eq!(next_page_number(19, 1, Some(1_000), 50, 50, None), Some(20));
    assert_eq!(next_page_number(20, 1, Some(1_000), 50, 50, None), None);
    // Pages counted from zero reach the same cap one page number sooner.
    assert_eq!(next_page_number(8, 0, Some(1_000), 100, 100, None), Some(9));
    assert_eq!(next_page_number(9, 0, Some(1_000), 100, 100, None), None);
    assert_eq!(
        next_page_number(u32::MAX, 1, Some(u32::MAX), 100, 100, None),
        None
    );
}

#[test]
fn a_page_counts_its_items_before_dropping_any_without_an_id() {
    // Whether a page came back full is about the provider's page size, not
    // the number of records kept from it.
    let payload = json!({ "messages": [{ "subject": "no id" }, { "id": "m1" }] });
    assert_eq!(item_count(&payload, &SPEC), 2);
    assert_eq!(page_from(&payload, &SPEC).records.len(), 1);
    assert_eq!(item_count(&json!({}), &SPEC), 0);
}

#[test]
fn a_github_window_narrows_the_query_it_is_given() {
    let mut arguments = json!({ "q": "involves:@me" });
    DepthWindow::GithubUpdatedSince.apply(&mut arguments, 30);
    let query = arguments["q"].as_str().unwrap();
    assert!(query.starts_with("involves:@me updated:>="), "{query}");
}

#[test]
fn a_github_window_without_a_query_leaves_the_read_alone() {
    // An `updated:` term on its own would search every repository on GitHub.
    let mut arguments = json!({ "per_page": 50 });
    DepthWindow::GithubUpdatedSince.apply(&mut arguments, 30);
    assert_eq!(arguments, json!({ "per_page": 50 }));
}
