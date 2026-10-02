//! Unit tests for URL encoding of bus-supplied values.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{comma_joined, encode};

#[test]
fn leaves_unreserved_characters_alone() {
    // Real Composio ids contain these. Encoding them produces a different
    // string from the one the backend stored, and the lookup misses.
    assert_eq!(encode("conn_9"), "conn_9");
    assert_eq!(encode("ca-1"), "ca-1");
    assert_eq!(encode("a.b~c"), "a.b~c");
    assert_eq!(encode("GMAIL_NEW_GMAIL_MESSAGE"), "GMAIL_NEW_GMAIL_MESSAGE");
}

#[test]
fn escapes_a_separator_that_would_change_the_request() {
    // A `/` would address a different endpoint; an `&` would forge a parameter.
    assert_eq!(encode("a/b"), "a%2Fb");
    assert_eq!(encode("a&b=c"), "a%26b%3Dc");
    assert_eq!(encode("a?b"), "a%3Fb");
    assert_eq!(encode("a b"), "a%20b");
}

#[test]
fn trims_before_encoding() {
    assert_eq!(encode("  conn_9  "), "conn_9");
}

#[test]
fn joins_values_with_commas() {
    let values = vec!["gmail".to_string(), "notion".to_string()];
    assert_eq!(comma_joined(&values).as_deref(), Some("gmail,notion"));
}

#[test]
fn drops_blank_values_from_a_join() {
    let values = vec!["gmail".to_string(), "  ".to_string(), String::new()];
    assert_eq!(comma_joined(&values).as_deref(), Some("gmail"));
}

#[test]
fn omits_the_parameter_entirely_when_nothing_survives() {
    // An empty parameter reads as "match nothing" on several endpoints, which
    // is the opposite of the "no filter" the caller meant.
    assert!(comma_joined(&[]).is_none());
    assert!(comma_joined(&["  ".to_string()]).is_none());
}

#[test]
fn encodes_each_joined_value_but_not_the_commas() {
    let values = vec!["a b".to_string(), "c&d".to_string()];
    assert_eq!(comma_joined(&values).as_deref(), Some("a%20b,c%26d"));
}
