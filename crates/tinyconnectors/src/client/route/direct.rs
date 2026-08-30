//! The direct route: Composio's own v3 API with a user-supplied key.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use super::Route;
use crate::client::Transport;
use crate::{
    ComposioAuthorizeResponse, ComposioConnection, ComposioConnectionsResponse,
    ComposioDeleteResponse, ComposioToolkitsResponse, Error, Result,
};

/// Composio's own API base. The direct route talks to this, not to us.
pub const COMPOSIO_API_BASE: &str = "https://backend.composio.dev/api/v3";

/// Consecutive rejections of the same key before the gate closes.
const INVALID_API_KEY_THRESHOLD: u32 = 3;

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

        let connect_url = field(&value, &["redirectUrl", "redirect_url", "connectUrl"]).ok_or(
            Error::Decode {
                path: path.to_string(),
                message: "link response carried no redirect URL".to_string(),
            },
        )?;

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

    async fn delete_connection(&self, _connection_id: &str) -> Result<ComposioDeleteResponse> {
        // The proxy route's delete also clears memory sourced from the
        // connection, which is a TinyHumans concern Composio knows nothing
        // about. Wiring a bare v3 delete here would silently drop that half and
        // leave the user's synced content behind after they disconnected.
        Err(Error::UnsupportedByRoute {
            route: self.name(),
            member: "DeleteConnection",
        })
    }
}

#[cfg(test)]
#[path = "direct_test.rs"]
mod test;
