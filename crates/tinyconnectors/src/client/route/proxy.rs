//! The backend-proxied route.

use std::sync::Arc;

use async_trait::async_trait;
use serde::de::DeserializeOwned;

use super::Route;
use crate::client::Transport;
use crate::{
    ComposioAuthorizeResponse, ComposioConnectionsResponse, ComposioDeleteResponse,
    ComposioExecuteResponse, ComposioToolkitsResponse, ComposioToolsResponse, Error, Result,
};

/// Reaches Composio through the `TinyHumans` backend.
///
/// The backend answers in this crate's envelopes already — they were defined
/// from its responses — so this route is paths and nothing else. That is the
/// point of preferring it: the allowlist, the margin, and the HMAC-verified
/// webhook fan-out all live on the far side.
#[derive(Debug, Clone)]
pub struct ProxyRoute {
    transport: Arc<dyn Transport>,
}

impl ProxyRoute {
    /// Build a proxy route over `transport`.
    #[must_use]
    pub fn new(transport: Arc<dyn Transport>) -> Self {
        Self { transport }
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        decode(path, self.transport.get(path).await?)
    }
}

/// Join non-empty, trimmed, percent-encoded values for a query parameter.
///
/// Encoding matters: a tag or toolkit slug reaching this from a bus call is not
/// guaranteed to be URL-safe, and an unencoded `&` would forge a parameter.
fn comma_joined(values: &[String]) -> Option<String> {
    let joined = values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(|value| {
            percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC)
                .to_string()
        })
        .collect::<Vec<_>>()
        .join(",");
    (!joined.is_empty()).then_some(joined)
}

fn decode<T: DeserializeOwned>(path: &str, value: serde_json::Value) -> Result<T> {
    serde_json::from_value(value).map_err(|error| Error::Decode {
        path: path.to_string(),
        message: error.to_string(),
    })
}

#[async_trait]
impl Route for ProxyRoute {
    fn name(&self) -> &'static str {
        "proxy"
    }

    async fn list_toolkits(&self) -> Result<ComposioToolkitsResponse> {
        tracing::debug!("[connectors][proxy] list_toolkits");
        self.get("/agent-integrations/composio/toolkits").await
    }

    async fn list_connections(&self) -> Result<ComposioConnectionsResponse> {
        tracing::debug!("[connectors][proxy] list_connections");
        self.get("/agent-integrations/composio/connections").await
    }

    async fn authorize(
        &self,
        toolkit: &str,
        body: &serde_json::Value,
    ) -> Result<ComposioAuthorizeResponse> {
        tracing::debug!(toolkit = %toolkit, "[connectors][proxy] authorize");
        let path = "/agent-integrations/composio/authorize";
        decode(path, self.transport.post(path, body).await?)
    }

    async fn list_tools(
        &self,
        toolkits: &[String],
        tags: &[String],
    ) -> Result<ComposioToolsResponse> {
        let mut query: Vec<String> = Vec::new();
        if let Some(joined) = comma_joined(toolkits) {
            query.push(format!("toolkits={joined}"));
        }
        if let Some(joined) = comma_joined(tags) {
            query.push(format!("tags={joined}"));
        }

        let path = if query.is_empty() {
            "/agent-integrations/composio/tools".to_string()
        } else {
            format!("/agent-integrations/composio/tools?{}", query.join("&"))
        };
        tracing::debug!(path = %path, "[connectors][proxy] list_tools");
        self.get(&path).await
    }

    async fn execute(
        &self,
        tool: &str,
        arguments: &serde_json::Value,
        connection_id: Option<&str>,
    ) -> Result<ComposioExecuteResponse> {
        tracing::debug!(tool = %tool, "[connectors][proxy] execute");
        let mut body = serde_json::json!({ "tool": tool, "arguments": arguments });
        if let Some(connection_id) = connection_id {
            body["connectionId"] = serde_json::Value::String(connection_id.to_string());
        }
        let path = "/agent-integrations/composio/execute";
        decode(path, self.transport.post(path, &body).await?)
    }

    async fn delete_connection(&self, connection_id: &str) -> Result<ComposioDeleteResponse> {
        tracing::debug!(connection_id = %connection_id, "[connectors][proxy] delete_connection");
        let path = format!("/agent-integrations/composio/connections/{connection_id}");
        decode(&path, self.transport.delete(&path).await?)
    }
}
