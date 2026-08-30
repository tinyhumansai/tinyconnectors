//! Unit tests for identity field extraction.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;

use super::pick;

#[test]
fn takes_the_first_path_that_matches() {
    // Order is how a provider says "prefer the display name, fall back to the
    // login" without writing the fallback out by hand.
    let payload = json!({ "login": "octocat", "name": "The Octocat" });
    assert_eq!(
        pick(&payload, &["name", "login"]).as_deref(),
        Some("The Octocat")
    );
    assert_eq!(
        pick(&payload, &["missing", "login"]).as_deref(),
        Some("octocat")
    );
}

#[test]
fn walks_a_dotted_path() {
    let payload = json!({ "person": { "email": "user@example.com" } });
    assert_eq!(
        pick(&payload, &["person.email"]).as_deref(),
        Some("user@example.com")
    );
}

#[test]
fn treats_an_empty_string_as_absent() {
    // "not reported" and "reported empty" are the same to a caller picking a
    // label, and an empty string renders as a blank account.
    let payload = json!({ "name": "   ", "login": "octocat" });
    assert_eq!(
        pick(&payload, &["name", "login"]).as_deref(),
        Some("octocat")
    );
}

#[test]
fn trims_what_it_finds() {
    let payload = json!({ "name": "  The Octocat  " });
    assert_eq!(pick(&payload, &["name"]).as_deref(), Some("The Octocat"));
}

#[test]
fn ignores_a_value_that_is_not_a_string() {
    let payload = json!({ "id": 42, "login": "octocat" });
    assert_eq!(pick(&payload, &["id", "login"]).as_deref(), Some("octocat"));
}

#[test]
fn is_none_when_nothing_matches() {
    assert!(pick(&json!({}), &["name", "login"]).is_none());
    assert!(pick(&json!({ "a": { "b": 1 } }), &["a.c"]).is_none());
    assert!(pick(&json!("a string"), &["name"]).is_none());
}
