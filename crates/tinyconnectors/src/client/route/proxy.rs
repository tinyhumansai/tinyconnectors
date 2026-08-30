//! The backend-proxied route.

use std::sync::Arc;

use async_trait::async_trait;
use serde::de::DeserializeOwned;

use super::Route;
use crate::client::Transport;
use crate::{
    ComposioActiveTriggersResponse, ComposioAuthorizeResponse, ComposioAvailableTriggersResponse,
    ComposioConnectionsResponse, ComposioCreateTriggerResponse, ComposioDeleteResponse,
    ComposioDisableTriggerResponse, ComposioEnableTriggerResponse, ComposioExecuteResponse,
    ComposioGithubReposResponse, ComposioToolkitsResponse, ComposioToolsResponse, Error, Result,
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

/// Percent-encode one value that becomes part of a path or query.
///
/// Every id and slug here arrived over the bus. An unencoded `/` in a
/// connection id would reach a different endpoint entirely, and an unencoded
/// `&` in a toolkit would forge a query parameter.
fn encode(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value.trim(), percent_encoding::NON_ALPHANUMERIC)
        .to_string()
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
        let path = format!(
            "/agent-integrations/composio/connections/{}",
            encode(connection_id)
        );
        decode(&path, self.transport.delete(&path).await?)
    }

    async fn list_github_repos(
        &self,
        connection_id: Option<&str>,
    ) -> Result<ComposioGithubReposResponse> {
        let path = match connection_id {
            Some(id) => format!(
                "/agent-integrations/composio/github/repos?connectionId={}",
                encode(id)
            ),
            None => "/agent-integrations/composio/github/repos".to_string(),
        };
        tracing::debug!(path = %path, "[connectors][proxy] list_github_repos");
        self.get(&path).await
    }

    async fn list_available_triggers(
        &self,
        toolkit: &str,
        connection_id: Option<&str>,
    ) -> Result<ComposioAvailableTriggersResponse> {
        let mut path = format!(
            "/agent-integrations/composio/triggers/available?toolkit={}",
            encode(toolkit)
        );
        if let Some(id) = connection_id {
            path.push_str(&format!("&connectionId={}", encode(id)));
        }
        tracing::debug!(path = %path, "[connectors][proxy] list_available_triggers");
        self.get(&path).await
    }

    async fn list_triggers(
        &self,
        toolkit: Option<&str>,
    ) -> Result<ComposioActiveTriggersResponse> {
        let path = match toolkit {
            Some(toolkit) => format!(
                "/agent-integrations/composio/triggers?toolkit={}",
                encode(toolkit)
            ),
            None => "/agent-integrations/composio/triggers".to_string(),
        };
        tracing::debug!(path = %path, "[connectors][proxy] list_triggers");
        self.get(&path).await
    }

    async fn create_trigger(
        &self,
        slug: &str,
        connection_id: Option<&str>,
        trigger_config: Option<serde_json::Value>,
    ) -> Result<ComposioCreateTriggerResponse> {
        tracing::debug!(slug = %slug, "[connectors][proxy] create_trigger");
        let mut body = serde_json::json!({ "slug": slug });
        if let Some(id) = connection_id {
            body["connectionId"] = serde_json::Value::String(id.to_string());
        }
        if let Some(config) = trigger_config {
            body["triggerConfig"] = config;
        }
        let path = "/agent-integrations/composio/triggers";
        decode(path, self.transport.post(path, &body).await?)
    }

    async fn enable_trigger(
        &self,
        connection_id: &str,
        slug: &str,
        trigger_config: Option<serde_json::Value>,
    ) -> Result<ComposioEnableTriggerResponse> {
        tracing::debug!(
            slug = %slug,
            connection_id = %connection_id,
            "[connectors][proxy] enable_trigger"
        );
        let mut body = serde_json::json!({ "connectionId": connection_id, "slug": slug });
        if let Some(config) = trigger_config {
            body["triggerConfig"] = config;
        }
        let path = "/agent-integrations/composio/triggers";
        decode(path, self.transport.post(path, &body).await?)
    }

    async fn disable_trigger(&self, trigger_id: &str) -> Result<ComposioDisableTriggerResponse> {
        tracing::debug!(trigger_id = %trigger_id, "[connectors][proxy] disable_trigger");
        let path = format!(
            "/agent-integrations/composio/triggers/{}",
            encode(trigger_id)
        );
        decode(&path, self.transport.delete(&path).await?)
    }
}
