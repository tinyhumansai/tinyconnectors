//! The Composio backend's paths and envelopes.

use std::sync::Arc;

use super::route::Route;
use crate::{
    ComposioAuthorizeResponse, ComposioConnectionsResponse, ComposioDeleteResponse,
    ComposioToolkitsResponse, Error, Result,
};

/// Keys the backend derives itself. Letting a caller set them would let a tool
/// argument redirect the handoff to a different toolkit or credential.
const RESERVED_AUTHORIZE_KEYS: &[&str] = &["toolkit", "toolkit_version", "auth", "client_id"];

/// Gmail's read scope is not in Composio's default set, and a connection made
/// without it authorizes cleanly and then fails on the first read — hours later,
/// as a sync error nobody connects back to the handoff.
const GMAIL_REQUIRED_OAUTH_SCOPES: &[&str] = &["https://www.googleapis.com/auth/gmail.readonly"];

const OAUTH_SCOPES_FIELD: &str = "oauth_scopes";

/// The Composio operations, over whichever route the host selected.
///
/// This type holds the policy that is true regardless of route — argument
/// validation, the reserved-key refusal, the required-scope merge — and hands
/// the call to a [`Route`] that knows its own paths and response shapes. That
/// split is what keeps every caller above from branching on mode.
#[derive(Debug, Clone)]
pub struct ComposioClient {
    route: Arc<dyn Route>,
}

impl ComposioClient {
    /// Build a client over `route`.
    #[must_use]
    pub fn new(route: Arc<dyn Route>) -> Self {
        Self { route }
    }

    /// Which route this client was built on, `"proxy"` or `"direct"`.
    #[must_use]
    pub fn route_name(&self) -> &'static str {
        self.route.name()
    }

    /// List the toolkits the backend allowlist currently enables.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedByRoute`] on the direct route, which has no
    /// allowlist to report, and otherwise a transport or decode failure.
    pub async fn list_toolkits(&self) -> Result<ComposioToolkitsResponse> {
        self.route.list_toolkits().await
    }

    /// List the caller's connections, active or not.
    ///
    /// Non-active rows are included deliberately: the OAuth cleanup in
    /// [`crate::oauth`] exists to find them, and filtering here would hide the
    /// debris it is meant to clear.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Transport`] if the request fails, or [`Error::Decode`]
    /// if the response does not match the expected envelope.
    pub async fn list_connections(&self) -> Result<ComposioConnectionsResponse> {
        self.route.list_connections().await
    }

    /// Begin an OAuth handoff for `toolkit`.
    ///
    /// `extra_params` is merged into the request body for toolkits that need
    /// fields Composio would otherwise reject the authorization without — a
    /// `WhatsApp` Business account id, for instance. Keys the backend derives
    /// itself are refused rather than silently dropped.
    ///
    /// The returned URL is the handoff: the user opens it, and the connection
    /// row it names stays non-active until they finish.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authorize`] if `toolkit` is empty or `extra_params` is
    /// not an object or collides with a reserved key, [`Error::Transport`] if
    /// the request fails, and [`Error::Decode`] on an unexpected envelope.
    pub async fn authorize(
        &self,
        toolkit: &str,
        extra_params: Option<serde_json::Value>,
    ) -> Result<ComposioAuthorizeResponse> {
        let toolkit = toolkit.trim();
        if toolkit.is_empty() {
            return Err(Error::Authorize {
                toolkit: String::new(),
                message: "toolkit must not be empty".to_string(),
            });
        }
        tracing::debug!(
            toolkit = %toolkit,
            has_extra_params = extra_params.is_some(),
            "[connectors][composio] authorize"
        );

        let mut body = serde_json::Map::new();
        body.insert("toolkit".to_string(), toolkit.into());

        if let Some(extra) = extra_params {
            let extra = extra.as_object().ok_or_else(|| Error::Authorize {
                toolkit: toolkit.to_string(),
                message: "extra_params must be a JSON object".to_string(),
            })?;
            for (key, value) in extra {
                if RESERVED_AUTHORIZE_KEYS.contains(&key.as_str()) {
                    return Err(Error::Authorize {
                        toolkit: toolkit.to_string(),
                        message: format!("extra_params cannot override reserved key '{key}'"),
                    });
                }
                body.insert(key.clone(), value.clone());
            }
        }

        merge_required_oauth_scopes(&mut body, toolkit);

        self.route
            .authorize(toolkit, &serde_json::Value::Object(body))
            .await
    }

    /// Disconnect a connection.
    ///
    /// The backend checks that the caller owns the row before removing it, and
    /// reports how many memory chunks sourced from it went with it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Authorize`] if `connection_id` is empty,
    /// [`Error::UnsupportedByRoute`] on the direct route, and otherwise a
    /// transport or decode failure.
    pub async fn delete_connection(&self, connection_id: &str) -> Result<ComposioDeleteResponse> {
        let connection_id = connection_id.trim();
        if connection_id.is_empty() {
            return Err(Error::Authorize {
                toolkit: String::new(),
                message: "connection_id must not be empty".to_string(),
            });
        }
        tracing::debug!(
            connection_id = %connection_id,
            "[connectors][composio] delete_connection"
        );
        let path = format!("/agent-integrations/composio/connections/{connection_id}");
        let value = self.transport.delete(&path).await?;
        decode(&path, value)
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let value = self.transport.get(path).await?;
        decode(path, value)
    }

    async fn post<T: DeserializeOwned>(&self, path: &str, body: &serde_json::Value) -> Result<T> {
        let value = self.transport.post(path, body).await?;
        decode(path, value)
    }
}

fn decode<T: DeserializeOwned>(path: &str, value: serde_json::Value) -> Result<T> {
    serde_json::from_value(value).map_err(|error| Error::Decode {
        path: path.to_string(),
        message: error.to_string(),
    })
}

/// The scopes a toolkit needs beyond Composio's defaults, if any.
fn required_oauth_scopes_for_toolkit(toolkit: &str) -> &'static [&'static str] {
    match toolkit.trim().to_ascii_lowercase().as_str() {
        "gmail" => GMAIL_REQUIRED_OAUTH_SCOPES,
        _ => &[],
    }
}

/// Add any missing required scopes to the authorize body.
///
/// Additive on purpose: a caller that already asked for scopes keeps them, and
/// a duplicate is not appended. Nothing is ever removed — narrowing a caller's
/// request here would break a connection they were relying on.
fn merge_required_oauth_scopes(
    body: &mut serde_json::Map<String, serde_json::Value>,
    toolkit: &str,
) {
    let required = required_oauth_scopes_for_toolkit(toolkit);
    if required.is_empty() {
        return;
    }

    let existing = body
        .get(OAUTH_SCOPES_FIELD)
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut scopes = existing;
    for scope in required {
        let already_present = scopes
            .iter()
            .filter_map(serde_json::Value::as_str)
            .any(|present| present == *scope);
        if !already_present {
            scopes.push(serde_json::Value::String((*scope).to_string()));
        }
    }

    body.insert(OAUTH_SCOPES_FIELD.to_string(), scopes.into());
}
