//! The one place the execute retry policy lives.

use std::time::Duration;

use super::classify::{ComposioErrorClass, classify_composio_error, format_provider_error};
use super::prepare::prepare_execute_arguments;
use crate::client::route::Route;
use crate::{ComposioExecuteResponse, Result};

/// How long to wait before the single post-OAuth readiness retry.
///
/// Eight seconds: long enough for Composio's execution gateway to sync a
/// freshly-issued token, short enough that a genuinely broken connection
/// surfaces well inside an agent's turn budget.
pub const POST_OAUTH_RETRY_DELAY: Duration = Duration::from_secs(8);

/// Actions whose upstream rate limits are retried transparently.
///
/// A deliberately short list. Slack's history read is what bursty agent reads
/// actually trip, and its retry semantics are stable. For anything else,
/// stalling a turn for half a minute is worse than reporting the limit.
pub const RATE_LIMIT_RETRY_TOOLS: &[&str] = &["SLACK_FETCH_CONVERSATION_HISTORY"];

/// Attempts for an allow-listed rate-limited action, including the first.
pub const RATE_LIMIT_MAX_ATTEMPTS: u32 = 6;

const RATE_LIMIT_INITIAL_BACKOFF: Duration = Duration::from_secs(2);
const RATE_LIMIT_MAX_BACKOFF: Duration = Duration::from_secs(30);

/// The literal a Composio gateway returns while a new token propagates.
const POST_OAUTH_NOT_READY: &str = "connection error, try to authenticate";

/// Run `tool` against `route`, with argument preparation, both retry policies,
/// and error formatting.
///
/// A provider that answers with `successful: false` is **not** an `Err`: the
/// call reached the provider and got a real answer. The failure is reported in
/// the returned response's `error`, formatted for the user. `Err` is reserved
/// for never having got an answer at all.
///
/// # Errors
///
/// Returns [`crate::Error::InvalidArguments`] when local validation rejects the
/// call, and a transport or decode failure when the call could not complete.
pub async fn execute_action(
    route: &dyn Route,
    tool: &str,
    arguments: Option<serde_json::Value>,
    connection_id: Option<&str>,
) -> Result<ComposioExecuteResponse> {
    let tool = tool.trim();
    let prepared = prepare_execute_arguments(tool, arguments)?;
    let connection_id = connection_id.map(str::trim).filter(|id| !id.is_empty());

    tracing::debug!(
        tool = %tool,
        connection_id = ?connection_id,
        route = route.name(),
        "[connectors][execute] dispatching"
    );

    let response = with_rate_limit_retry(route, tool, &prepared, connection_id).await?;
    Ok(format_failure(tool, response))
}

/// Retry an allow-listed action while the upstream reports a rate limit.
async fn with_rate_limit_retry(
    route: &dyn Route,
    tool: &str,
    arguments: &serde_json::Value,
    connection_id: Option<&str>,
) -> Result<ComposioExecuteResponse> {
    let retryable = RATE_LIMIT_RETRY_TOOLS.contains(&tool);
    let mut delay = RATE_LIMIT_INITIAL_BACKOFF;

    for attempt in 1..=RATE_LIMIT_MAX_ATTEMPTS {
        // The readiness retry belongs to the first attempt only: after a
        // rate-limit backoff the token has long since propagated, and paying
        // another eight seconds to find that out would compound the wait.
        let response = if attempt == 1 {
            with_post_oauth_retry(route, tool, arguments, connection_id).await?
        } else {
            route.execute(tool, arguments, connection_id).await?
        };

        if response.successful || !retryable || attempt == RATE_LIMIT_MAX_ATTEMPTS {
            return Ok(response);
        }

        let reported = response.error.as_deref().unwrap_or_default();
        if classify_composio_error(tool, reported) != ComposioErrorClass::RateLimited {
            return Ok(response);
        }

        tracing::warn!(
            tool = %tool,
            attempt,
            max_attempts = RATE_LIMIT_MAX_ATTEMPTS,
            sleep_secs = delay.as_secs(),
            "[connectors][execute] upstream rate limit; backing off"
        );
        tokio::time::sleep(delay).await;
        delay = (delay * 2).min(RATE_LIMIT_MAX_BACKOFF);
    }

    // Unreachable: the final attempt returns through the guard above.
    route.execute(tool, arguments, connection_id).await
}

/// Retry exactly once when the gateway says a fresh connection is not ready.
async fn with_post_oauth_retry(
    route: &dyn Route,
    tool: &str,
    arguments: &serde_json::Value,
    connection_id: Option<&str>,
) -> Result<ComposioExecuteResponse> {
    let response = route.execute(tool, arguments, connection_id).await?;
    if !is_post_oauth_not_ready(&response) {
        return Ok(response);
    }

    tracing::info!(
        tool = %tool,
        sleep_secs = POST_OAUTH_RETRY_DELAY.as_secs(),
        "[connectors][execute] connection not ready yet; retrying once"
    );
    tokio::time::sleep(POST_OAUTH_RETRY_DELAY).await;

    // Returned verbatim whatever it is. A second failure is the real answer,
    // and retrying again would loop on a genuinely revoked connection.
    route.execute(tool, arguments, connection_id).await
}

fn is_post_oauth_not_ready(response: &ComposioExecuteResponse) -> bool {
    !response.successful
        && response
            .error
            .as_deref()
            .is_some_and(|error| error.to_ascii_lowercase().contains(POST_OAUTH_NOT_READY))
}

/// Rewrite a reported failure into a message the user can act on.
fn format_failure(tool: &str, response: ComposioExecuteResponse) -> ComposioExecuteResponse {
    if response.successful {
        return response;
    }
    let raw = response
        .error
        .clone()
        .unwrap_or_else(|| "provider reported failure".to_string());
    ComposioExecuteResponse {
        error: Some(format_provider_error(tool, &raw)),
        ..response
    }
}
