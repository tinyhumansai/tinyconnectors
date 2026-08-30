//! The direct route: Composio's own v3 API with a user-supplied key.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use super::Route;
use super::url::{comma_joined, encode};
use crate::client::Transport;
use crate::{
    ComposioActiveTriggersResponse, ComposioAuthorizeResponse, ComposioAvailableTriggersResponse,
    ComposioConnection, ComposioConnectionsResponse, ComposioCreateTriggerResponse,
    ComposioDeleteResponse, ComposioDisableTriggerResponse, ComposioEnableTriggerResponse,
    ComposioExecuteResponse, ComposioGithubReposResponse, ComposioToolkitsResponse,
    ComposioToolsResponse, Error, Result,
};

/// Composio's own API base. The direct route talks to this, not to us.
pub const COMPOSIO_API_BASE: &str = "https://backend.composio.dev/api/v3";

/// Consecutive rejections of the same key before the gate closes.
pub const INVALID_API_KEY_THRESHOLD: u32 = 3;

const GATED_MESSAGE: &str = "Composio rejected this API key repeatedly, so further direct-mode \
                             calls are paused. Enter a valid key to resume.";

/// Reaches Composio directly with a user-supplied `x-api-key`.
///
/// # Why this route reshapes responses
///
/// Composio's v3 API is not the shape the backend returns. Its
/// `/connected_accounts` items are not [`ComposioConnection`]s, and its link
/// response carries no connection id at all. Translating here is what lets
/// every caller above stay unaware of which route it is on.
///
/// # Why it counts failures
///
/// A revoked key fails identically forever, and the connection list is polled
/// every few seconds. Without the gate that is a fixed stream of doomed
/// requests against Composio for as long as the app is open. After
/// [`INVALID_API_KEY_THRESHOLD`] consecutive rejections the route stops asking
/// and returns [`Error::DirectAuthGated`]; a single success clears the count.
#[derive(Debug)]
pub struct DirectRoute {
    transport: Arc<dyn Transport>,
    entity_id: String,
    /// Failures counted per key fingerprint, never per key.
    ///
    /// Keyed by a hash so a diagnostic dump of this map cannot leak the key,
    /// and so replacing a bad key starts a fresh count rather than inheriting
    /// the old one's.
    failures: Mutex<HashMap<u64, u32>>,
    key_fingerprint: u64,
}

impl DirectRoute {
    /// Build a direct route over `transport`, acting as `entity_id`.
    ///
    /// `api_key` is used only to fingerprint the key for failure counting — the
    /// credential itself lives in the transport, which is what sends it.
    #[must_use]
    pub fn new(transport: Arc<dyn Transport>, api_key: &str, entity_id: impl Into<String>) -> Self {
        let entity_id = entity_id.into();
        let entity_id = if entity_id.trim().is_empty() {
            "default".to_string()
        } else {
            entity_id
        };
        Self {
            transport,
            entity_id,
            failures: Mutex::new(HashMap::new()),
            key_fingerprint: fingerprint(api_key),
        }
    }

    /// Refuse before calling when the key is already known bad.
    fn check_gate(&self) -> Result<()> {
        let failures = self
            .failures
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if failures.get(&self.key_fingerprint).copied().unwrap_or(0) >= INVALID_API_KEY_THRESHOLD {
            return Err(Error::DirectAuthGated {
                message: GATED_MESSAGE.to_string(),
            });
        }
        Ok(())
    }

    /// Record the outcome of a call, closing or clearing the gate.
    ///
    /// Only authentication failures count. A 500 or a dropped connection says
    /// nothing about the key, and counting it would gate a user whose key is
    /// fine because Composio had a bad afternoon.
    fn record(&self, outcome: &Result<serde_json::Value>) {
        let mut failures = self
            .failures
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match outcome {
            Ok(_) => {
                failures.remove(&self.key_fingerprint);
            }
            Err(error) if is_invalid_api_key(&error.to_string()) => {
                let count = failures.entry(self.key_fingerprint).or_insert(0);
                *count += 1;
                tracing::warn!(
                    consecutive = *count,
                    threshold = INVALID_API_KEY_THRESHOLD,
                    "[connectors][direct] Composio rejected the API key"
                );
            }
            Err(_) => {
                // Not an auth failure: leave the count alone. Clearing it here
                // would let an intermittent outage reset a genuinely bad key's
                // tally and reopen the gate on every blip.
            }
        }
    }

    /// Refuse a member this route cannot honestly serve.
    ///
    /// Every trigger member lands here. A trigger is a webhook subscription,
    /// and a webhook has to arrive somewhere: the proxy backend HMAC-verifies
    /// deliveries and fans them out over the user's sockets, while this module
    /// has no socket and no public endpoint. A direct-mode subscription would
    /// be created successfully and then deliver to nobody — a silent failure
    /// the user would only notice as an automation that never fires.
    fn unsupported(&self, member: &'static str) -> Error {
        Error::UnsupportedByRoute {
            route: self.name(),
            member,
        }
    }

    async fn call<F>(&self, request: F) -> Result<serde_json::Value>
    where
        F: std::future::Future<Output = Result<serde_json::Value>>,
    {
        self.check_gate()?;
        let outcome = request.await;
        self.record(&outcome);
        outcome
    }
}

fn fingerprint(api_key: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    api_key.trim().hash(&mut hasher);
    hasher.finish()
}

/// Whether a rendered failure says the key itself was refused.
fn is_invalid_api_key(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("invalid api key")
        || (lower.contains("401") && lower.contains("api key") && lower.contains("invalid"))
}

/// Read a string field, tolerating the object-wrapped form Composio also emits.
fn field(item: &serde_json::Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        match item.get(*key) {
            Some(serde_json::Value::String(value)) => return Some(value.clone()),
            Some(serde_json::Value::Object(nested)) => {
                for inner in ["slug", "id", "name", "key"] {
                    if let Some(serde_json::Value::String(value)) = nested.get(inner) {
                        return Some(value.clone());
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Translate one v3 `connected_accounts` item into a [`ComposioConnection`].
///
/// Defensive on every field: a row missing its toolkit or status is kept with
/// empty strings rather than dropped, and `is_active` treats an empty status as
/// inactive. A malformed row therefore shows up as not-connected — which is the
/// fail-safe direction — instead of disappearing and looking deleted.
fn connection_from_v3(item: &serde_json::Value) -> Option<ComposioConnection> {
    let id = field(item, &["id", "nanoid", "connectedAccountId"])?;
    Some(ComposioConnection {
        id,
        toolkit: field(item, &["toolkit", "appName", "toolkit_slug", "appUniqueId"])
            .unwrap_or_default(),
        status: field(item, &["status", "connectionStatus"]).unwrap_or_default(),
        created_at: field(item, &["createdAt", "created_at"]),
        account_email: field(item, &["accountEmail", "email"]),
        workspace: field(item, &["workspace", "teamName"]),
        username: field(item, &["username", "login", "screenName"]),
    })
}

#[async_trait]
impl Route for DirectRoute {
    fn name(&self) -> &'static str {
        "direct"
    }

    async fn list_toolkits(&self) -> Result<ComposioToolkitsResponse> {
        // Not an oversight. The allowlist is a property of the TinyHumans
        // backend: it decides which toolkits a given user may connect. Talking
        // to Composio directly there is no such list — the answer would be
        // "everything Composio offers", which is not what a caller asking for
        // an allowlist means. Returning an empty list would read as "you may
        // connect nothing", which is worse than a refusal.
        Err(Error::UnsupportedByRoute {
            route: self.name(),
            member: "ListToolkits",
        })
    }

    async fn list_connections(&self) -> Result<ComposioConnectionsResponse> {
        tracing::debug!("[connectors][direct] GET /connected_accounts");
        let value = self.call(self.transport.get("/connected_accounts")).await?;

        // v3 has returned both a bare array and `{ items: [...] }`.
        let items = value
            .get("items")
            .or_else(|| value.get("data"))
            .unwrap_or(&value);
        let connections = items
            .as_array()
            .map(|rows| rows.iter().filter_map(connection_from_v3).collect())
            .unwrap_or_default();

        Ok(ComposioConnectionsResponse { connections })
    }

    async fn authorize(
        &self,
        toolkit: &str,
        body: &serde_json::Value,
    ) -> Result<ComposioAuthorizeResponse> {
        tracing::debug!(toolkit = %toolkit, "[connectors][direct] POST /connected_accounts/link");
        let mut request = body.clone();
        if let Some(object) = request.as_object_mut() {
            object.insert("entity_id".to_string(), self.entity_id.clone().into());
        }

        let path = "/connected_accounts/link";
        let value = self.call(self.transport.post(path, &request)).await?;

        let connect_url =
            field(&value, &["redirectUrl", "redirect_url", "connectUrl"]).ok_or(Error::Decode {
                path: path.to_string(),
                message: "link response carried no redirect URL".to_string(),
            })?;

        Ok(ComposioAuthorizeResponse {
            connect_url,
            // The v3 link response has no stable connection id: the row is
            // created lazily when the user finishes OAuth on Composio's hosted
            // page. Callers poll `ListConnections` to see it appear, which they
            // must do on the proxy route too — the row is not active at this
            // point either way.
            connection_id: field(&value, &["connectedAccountId", "id"]).unwrap_or_default(),
        })
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
            "/tools".to_string()
        } else {
            format!("/tools?{}", query.join("&"))
        };

        tracing::debug!(path = %path, "[connectors][direct] list_tools");
        let value = self.call(self.transport.get(&path)).await?;
        Ok(ComposioToolsResponse {
            tools: tool_schemas_from_v3(&value),
        })
    }

    async fn execute(
        &self,
        tool: &str,
        arguments: &serde_json::Value,
        connection_id: Option<&str>,
    ) -> Result<ComposioExecuteResponse> {
        // v3 puts the action slug in the path, not the body. The slug is
        // upper-snake-case and comes from a catalog rather than free text, but
        // it still reaches here over the bus, so it is encoded before it
        // becomes part of a URL.
        let path = format!("/tools/execute/{}", encode(tool));
        tracing::debug!(tool = %tool, "[connectors][direct] execute");

        let mut body = serde_json::json!({
            "arguments": arguments,
            "entity_id": self.entity_id,
        });
        if let Some(connection_id) = connection_id {
            body["connected_account_id"] = serde_json::Value::String(connection_id.to_string());
        }

        let raw = self.call(self.transport.post(&path, &body)).await?;
        Ok(execute_response_from_v3(raw))
    }

    async fn delete_connection(&self, _connection_id: &str) -> Result<ComposioDeleteResponse> {
        // The proxy route's delete also clears memory sourced from the
        // connection, which is a `TinyHumans` concern Composio knows nothing
        // about. Wiring a bare v3 delete here would silently drop that half and
        // leave the user's synced content behind after they disconnected.
        Err(self.unsupported("DeleteConnection"))
    }

    async fn list_github_repos(
        &self,
        _connection_id: Option<&str>,
    ) -> Result<ComposioGithubReposResponse> {
        // Only ever used to pick a repository for a GitHub-scoped trigger, and
        // triggers are unavailable here. Offering the picker without the thing
        // it picks for would be a dead end.
        Err(self.unsupported("ListGithubRepos"))
    }

    async fn list_available_triggers(
        &self,
        _toolkit: &str,
        _connection_id: Option<&str>,
    ) -> Result<ComposioAvailableTriggersResponse> {
        Err(self.unsupported("ListAvailableTriggers"))
    }

    async fn list_triggers(
        &self,
        _toolkit: Option<&str>,
    ) -> Result<ComposioActiveTriggersResponse> {
        Err(self.unsupported("ListTriggers"))
    }

    async fn create_trigger(
        &self,
        _slug: &str,
        _connection_id: Option<&str>,
        _trigger_config: Option<serde_json::Value>,
    ) -> Result<ComposioCreateTriggerResponse> {
        Err(self.unsupported("CreateTrigger"))
    }

    async fn enable_trigger(
        &self,
        _connection_id: &str,
        _slug: &str,
        _trigger_config: Option<serde_json::Value>,
    ) -> Result<ComposioEnableTriggerResponse> {
        Err(self.unsupported("EnableTrigger"))
    }

    async fn disable_trigger(&self, _trigger_id: &str) -> Result<ComposioDisableTriggerResponse> {
        Err(self.unsupported("DisableTrigger"))
    }
}

/// Pull tool schemas out of a v3 listing, which nests them under `items`.
fn tool_schemas_from_v3(value: &serde_json::Value) -> Vec<crate::ComposioToolSchema> {
    let items = value
        .get("items")
        .or_else(|| value.get("data"))
        .or_else(|| value.get("tools"))
        .unwrap_or(value);
    let Some(rows) = items.as_array() else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            // A v3 row is the function itself, not the `{type, function}`
            // envelope a model expects, so it is wrapped here.
            let name = field(row, &["slug", "name"])?;
            Some(crate::ComposioToolSchema {
                kind: "function".to_string(),
                function: crate::ComposioToolFunction {
                    name,
                    description: field(row, &["description"]),
                    parameters: row
                        .get("input_parameters")
                        .or_else(|| row.get("parameters"))
                        .cloned(),
                    output_parameters: row.get("output_parameters").cloned(),
                },
            })
        })
        .collect()
}

/// Reshape a v3 execute result into the envelope the proxy returns.
///
/// Two differences are load-bearing. `successful` defaults to *true* when the
/// response carries no verdict at all: v3 answers a plain result for some
/// actions, and defaulting to failure would report a completed action as
/// broken. And `cost_usd` is zero because direct mode carries no billing
/// margin — the user is paying Composio, not us.
fn execute_response_from_v3(raw: serde_json::Value) -> ComposioExecuteResponse {
    let successful = raw
        .get("successful")
        .or_else(|| raw.get("success"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    let error = raw
        .get("error")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let data = raw.get("data").cloned().unwrap_or(raw);

    ComposioExecuteResponse {
        data,
        successful,
        error,
        cost_usd: 0.0,
        // The proxy backend renders compact markdown for known tools; Composio
        // does not. Callers fall back to `data`.
        markdown_formatted: None,
    }
}

#[cfg(test)]
#[path = "direct_test.rs"]
mod test;
