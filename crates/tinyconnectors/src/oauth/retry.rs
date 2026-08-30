//! Backing off when the OAuth host rate-limits an authorize call.

use std::time::Duration;

use super::status::{
    is_authorize_rate_limited, is_meta_oauth_toolkit, meta_oauth_rate_limit_message,
};
use crate::{Error, Result};

/// How many times an authorize call is attempted before the rate limit is
/// reported to the caller.
pub const AUTHORIZE_RATE_LIMIT_MAX_ATTEMPTS: u32 = 3;

const AUTHORIZE_RATE_LIMIT_INITIAL_BACKOFF: Duration = Duration::from_secs(5);
const AUTHORIZE_RATE_LIMIT_MAX_BACKOFF: Duration = Duration::from_secs(60);

/// Replace a Meta rate-limit failure with guidance the user can act on.
///
/// Non-Meta toolkits and non-rate-limit failures pass through untouched: the
/// original message is more useful than a generic one, and only Meta's flows
/// have the account-type causes the guidance describes.
#[must_use]
pub fn wrap_authorize_rate_limit_error(toolkit: &str, error: Error) -> Error {
    let rendered = error.to_string();
    if is_meta_oauth_toolkit(toolkit) && is_authorize_rate_limited(&rendered) {
        Error::OauthRateLimited {
            toolkit: toolkit.trim().to_string(),
            message: meta_oauth_rate_limit_message(toolkit),
        }
    } else {
        error
    }
}

/// Run `attempt` until it succeeds, is rejected for a reason other than rate
/// limiting, or [`AUTHORIZE_RATE_LIMIT_MAX_ATTEMPTS`] is reached.
///
/// The backoff doubles from five seconds and is capped at a minute. It is
/// generic over the attempt rather than taking a client so the policy can be
/// tested — and reused by direct mode — without an HTTP round trip.
///
/// # Errors
///
/// Returns the last failure `attempt` produced. A rate limit that outlives
/// every attempt is returned as it arrived; call
/// [`wrap_authorize_rate_limit_error`] to turn it into user-facing guidance.
pub async fn authorize_with_rate_limit_retry<T, F, Fut>(mut attempt: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut delay = AUTHORIZE_RATE_LIMIT_INITIAL_BACKOFF;

    // Every attempt but the last. Written this way rather than as a loop over
    // all of them so there is no unreachable "ran out of attempts" arm at the
    // end: the final attempt below is the one that reports.
    for attempt_number in 1..AUTHORIZE_RATE_LIMIT_MAX_ATTEMPTS {
        match attempt().await {
            Ok(value) => return Ok(value),
            Err(error) => {
                if !is_authorize_rate_limited(&error.to_string()) {
                    return Err(error);
                }
                tracing::warn!(
                    attempt = attempt_number,
                    max_attempts = AUTHORIZE_RATE_LIMIT_MAX_ATTEMPTS,
                    sleep_secs = delay.as_secs(),
                    "[connectors][oauth] authorize rate-limited; backing off"
                );
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(AUTHORIZE_RATE_LIMIT_MAX_BACKOFF);
            }
        }
    }

    // The last attempt, reported whatever it produces.
    attempt().await
}
