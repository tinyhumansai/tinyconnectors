//! Unit tests for the OAuth handoff policy.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::Cell;

use super::*;
use crate::Error;

#[test]
fn recognizes_meta_toolkits_regardless_of_casing_or_padding() {
    for toolkit in ["instagram", "Instagram", " FACEBOOK ", "facebook"] {
        assert!(
            is_meta_oauth_toolkit(toolkit),
            "{toolkit:?} is Meta-hosted"
        );
    }
}

#[test]
fn does_not_treat_other_toolkits_as_meta_hosted() {
    for toolkit in ["gmail", "slack", "notion", ""] {
        assert!(!is_meta_oauth_toolkit(toolkit), "{toolkit:?} is not Meta");
    }
}

#[test]
fn treats_pending_initiated_and_initializing_as_in_flight() {
    for status in ["PENDING", "initiated", " Initializing "] {
        assert!(is_inflight_oauth_status(status), "{status:?} is in flight");
    }
}

#[test]
fn does_not_treat_a_finished_status_as_in_flight() {
    for status in ["ACTIVE", "FAILED", "EXPIRED", ""] {
        assert!(!is_inflight_oauth_status(status));
    }
}

#[test]
fn clears_in_flight_and_failed_rows_but_never_an_active_one() {
    for status in [
        "PENDING",
        "INITIATED",
        "INITIALIZING",
        "FAILED",
        "ERROR",
        "EXPIRED",
    ] {
        assert!(is_clearable_oauth_status(status), "{status:?} is clearable");
    }
    for status in ["ACTIVE", "CONNECTED", ""] {
        assert!(
            !is_clearable_oauth_status(status),
            "{status:?} must be kept"
        );
    }
}

#[test]
fn detects_every_observed_spelling_of_a_rate_limit() {
    for message in [
        "HTTP 429 from upstream",
        "Too Many Requests",
        "upstream rate limit exceeded",
        "error: rate_limit_hit",
        "RateLimited",
    ] {
        assert!(is_authorize_rate_limited(message), "{message:?}");
    }
}

#[test]
fn does_not_mistake_an_ordinary_failure_for_a_rate_limit() {
    for message in ["invalid toolkit", "403 forbidden", "connection reset"] {
        assert!(!is_authorize_rate_limited(message), "{message:?}");
    }
}

#[test]
fn gives_instagram_and_facebook_their_own_account_hints() {
    let instagram = meta_oauth_rate_limit_message("instagram");
    assert!(instagram.contains("Business or Creator"));

    let facebook = meta_oauth_rate_limit_message("facebook");
    assert!(facebook.contains("Page or Business Manager"));

    // A toolkit with no known cause gets the backoff advice and nothing made up.
    let other = meta_oauth_rate_limit_message("gmail");
    assert!(other.contains("Wait a few"));
    assert!(!other.contains("Business"));
}

#[test]
fn wraps_only_a_meta_rate_limit() {
    let rate_limited = || Error::Authorize {
        toolkit: "instagram".into(),
        message: "HTTP 429 too many requests".into(),
    };

    let wrapped = wrap_authorize_rate_limit_error("instagram", rate_limited());
    assert!(matches!(wrapped, Error::OauthRateLimited { .. }));
    assert!(wrapped.to_string().contains("Business or Creator"));

    // Same failure, non-Meta toolkit: the original message is more useful.
    let untouched = wrap_authorize_rate_limit_error("gmail", rate_limited());
    assert!(matches!(untouched, Error::Authorize { .. }));

    // Meta toolkit, but not a rate limit.
    let other = wrap_authorize_rate_limit_error(
        "facebook",
        Error::Authorize {
            toolkit: "facebook".into(),
            message: "invalid scope".into(),
        },
    );
    assert!(matches!(other, Error::Authorize { .. }));
}

#[tokio::test(start_paused = true)]
async fn returns_the_first_success_without_retrying() {
    let calls = Cell::new(0u32);
    let result = authorize_with_rate_limit_retry(|| {
        calls.set(calls.get() + 1);
        async { Ok::<_, Error>("connect-url") }
    })
    .await;

    assert_eq!(result.unwrap(), "connect-url");
    assert_eq!(calls.get(), 1);
}

#[tokio::test(start_paused = true)]
async fn retries_a_rate_limit_and_succeeds_on_a_later_attempt() {
    let calls = Cell::new(0u32);
    let result = authorize_with_rate_limit_retry(|| {
        calls.set(calls.get() + 1);
        let attempt = calls.get();
        async move {
            if attempt < 3 {
                Err(Error::Authorize {
                    toolkit: "instagram".into(),
                    message: "429 too many requests".into(),
                })
            } else {
                Ok("connect-url")
            }
        }
    })
    .await;

    assert_eq!(result.unwrap(), "connect-url");
    assert_eq!(calls.get(), 3);
}

#[tokio::test(start_paused = true)]
async fn gives_up_after_the_attempt_limit() {
    let calls = Cell::new(0u32);
    let result: Result<&str> = authorize_with_rate_limit_retry(|| {
        calls.set(calls.get() + 1);
        async {
            Err(Error::Authorize {
                toolkit: "instagram".into(),
                message: "429 too many requests".into(),
            })
        }
    })
    .await;

    assert!(result.is_err());
    assert_eq!(calls.get(), AUTHORIZE_RATE_LIMIT_MAX_ATTEMPTS);
}

#[tokio::test(start_paused = true)]
async fn does_not_retry_a_failure_that_is_not_a_rate_limit() {
    let calls = Cell::new(0u32);
    let result: Result<&str> = authorize_with_rate_limit_retry(|| {
        calls.set(calls.get() + 1);
        async {
            Err(Error::Authorize {
                toolkit: "gmail".into(),
                message: "invalid toolkit".into(),
            })
        }
    })
    .await;

    assert!(result.is_err());
    assert_eq!(calls.get(), 1, "a permanent failure must not be retried");
}
