//! Stateless direct reads: Composio's v3 list endpoints with a credential
//! supplied by the caller.
//!
//! The configured route is one shared, replaceable thing. A read that carries
//! its own credential builds a transport, makes one call, and drops everything,
//! so several credentials can be served from one module at once and an unsaved
//! key can be checked without being kept.
//!
//! # Failure messages
//!
//! A failure is returned as the message a user ends up reading, without the
//! `request to <path> failed:` wrapper [`crate::Error`] adds. The wording is
//! fixed because hosts act on it: `HTTP 401: Invalid API key` is how a rejected
//! key is told apart from an outage.
//!
//! - `Composio v3 connected_accounts failed: HTTP <status>[: <message>]`
//! - `Composio v3 list_tool_schemas: HTTP <status>[: <message>]`
//! - `Failed to decode Composio v3 <connected_accounts|tools> response: ..`
//! - a failure before any response reads `error sending request for url (..)`

use tinyconnectors_bus::{
    ComposioConnectionsResponse, ComposioDirectCredential, ComposioToolsResponse,
};

use super::route::{COMPOSIO_API_BASE, DirectRoute, Route};
use super::{HttpTransport, Transport};
use crate::Error;

/// How one read labels its failures.
struct Labels {
    /// Prefix for a failing status.
    status: &'static str,
    /// Prefix for a body that is not JSON.
    decode: &'static str,
}

const CONNECTIONS: Labels = Labels {
    status: "Composio v3 connected_accounts failed",
    decode: "Failed to decode Composio v3 connected_accounts response",
};

const TOOLS: Labels = Labels {
    status: "Composio v3 list_tool_schemas",
    decode: "Failed to decode Composio v3 tools response",
};

/// List connections as `credential`.
///
/// # Errors
///
/// The failure message, in the wording described in the module docs.
pub async fn list_connections(
    credential: &ComposioDirectCredential,
) -> Result<ComposioConnectionsResponse, String> {
    tracing::debug!("[connectors][direct-read] list_connections");
    let route = route_for(credential)?;
    route
        .list_connections()
        .await
        .map_err(|error| render(error, &CONNECTIONS))
}

/// List tools as `credential`, for `toolkits` and `tags`.
///
/// # Errors
///
/// The failure message, in the wording described in the module docs.
pub async fn list_tools(
    credential: &ComposioDirectCredential,
    toolkits: &[String],
    tags: &[String],
) -> Result<ComposioToolsResponse, String> {
    tracing::debug!(
        toolkits = toolkits.len(),
        tags = tags.len(),
        "[connectors][direct-read] list_tools"
    );
    let route = route_for(credential)?;
    route
        .list_tools(toolkits, tags)
        .await
        .map_err(|error| render(error, &TOOLS))
}

/// A throwaway direct route over `credential`.
fn route_for(credential: &ComposioDirectCredential) -> Result<DirectRoute, String> {
    let api_key = credential.api_key.trim();
    if api_key.is_empty() {
        return Err("composio direct api key must not be empty".to_string());
    }
    let base_url = credential
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(COMPOSIO_API_BASE);
    let transport = HttpTransport::api_key(base_url, api_key)
        .and_then(|transport| transport.with_network(credential.transport.as_ref()))
        .map_err(|error| error.to_string())?;
    let transport: std::sync::Arc<dyn Transport> = std::sync::Arc::new(transport);
    Ok(DirectRoute::new(
        transport,
        api_key,
        credential.entity_id.clone().unwrap_or_default(),
    ))
}

fn render(error: Error, labels: &Labels) -> String {
    match error {
        Error::Transport { message, .. } if message.starts_with("HTTP ") => {
            format!("{}: {message}", labels.status)
        }
        Error::Transport { message, .. } => message,
        Error::Decode { message, .. } => {
            format!("{}: error decoding response body: {message}", labels.decode)
        }
        other => other.to_string(),
    }
}

#[cfg(test)]
#[path = "direct_read_tests.rs"]
mod test;
