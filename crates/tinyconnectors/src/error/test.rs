//! Unit tests for the crate-wide error type.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

#[test]
fn renders_a_human_readable_message() {
    let error = Error::Authorize {
        toolkit: "gmail".into(),
        message: "invalid scope".into(),
    };
    assert_eq!(
        error.to_string(),
        "authorize failed for gmail: invalid scope"
    );
}

#[test]
fn a_rate_limit_renders_as_the_guidance_alone() {
    // The message is written for a user, so it must not be prefixed with
    // machinery about which toolkit and which call failed.
    let error = Error::OauthRateLimited {
        toolkit: "instagram".into(),
        message: "Meta is temporarily rate-limiting instagram sign-in.".into(),
    };
    assert_eq!(
        error.to_string(),
        "Meta is temporarily rate-limiting instagram sign-in."
    );
}

#[test]
fn is_a_standard_error() {
    fn assert_error<E: std::error::Error>(_: &E) {}

    assert_error(&Error::Authorize {
        toolkit: "gmail".into(),
        message: "invalid scope".into(),
    });
}
