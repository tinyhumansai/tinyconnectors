//! Unit tests for the crate-wide error type.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::Error;

#[test]
fn a_store_failure_names_the_key_and_repeats_the_host_message() {
    let error = Error::Store {
        key: "gmail:conn_1".into(),
        message: "database is locked".into(),
    };
    assert_eq!(
        error.to_string(),
        "sync state store failed for gmail:conn_1: database is locked"
    );
}

#[test]
fn a_decode_failure_is_distinguishable_from_a_store_failure() {
    // The distinction is what tells a caller whether retrying could help.
    let error = Error::Decode {
        key: "gmail:conn_1".into(),
        message: "missing field `toolkit`".into(),
    };
    assert!(error.to_string().contains("did not match its shape"));
}

#[test]
fn is_a_standard_error() {
    fn assert_error<E: std::error::Error>(_: &E) {}
    assert_error(&Error::Store {
        key: "k".into(),
        message: "m".into(),
    });
}
