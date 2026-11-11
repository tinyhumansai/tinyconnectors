//! The direct route: Composio's own v3 API with a user-supplied key.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use super::Route;
use super::url::{comma_joined, encode, segment};

#[path = "direct_triggers.rs"]
mod triggers;
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

/// Most pages of one v3 listing the route will follow.
const MAX_PAGES: usize = 20;

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
    /// Whether the host receives webhook deliveries itself, which is what
    /// makes the trigger members safe to serve. See [`Self::with_webhooks`].
    webhooks: bool,
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
            webhooks: false,
        }
    }

    /// Serve the trigger members, for a host that receives Composio's webhook
    /// deliveries at its own public endpoint.
    ///
    /// Off by default, and every trigger member then refuses with
    /// [`Error::UnsupportedByRoute`]: a subscription whose deliveries reach no
    /// endpoint is created successfully and then fires into the void. A host
    /// turns it on only once it has pointed the Composio project's webhook at
    /// an endpoint it serves, and verifies what arrives there (the contract
    /// crate's `webhook` feature does that part).
    ///
    /// With it on, subscriptions are made and listed as the route's entity
    /// alone: a connection this entity is not known to own is refused with
    /// [`Error::ConnectionNotOwned`], and a subscription that is not the
    /// entity's cannot be removed ([`Error::TriggerNotOwned`]).
    #[must_use]
    pub fn with_webhooks(mut self, webhooks: bool) -> Self {
        self.webhooks = webhooks;
        self
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
            Err(error) if is_invalid_api_key(error) => {
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
    /// Every trigger member lands here unless the host opted in with
    /// [`Self::with_webhooks`]. A trigger is a webhook subscription, and a
    /// webhook has to arrive somewhere: the proxy backend HMAC-verifies
    /// deliveries and fans them out over the user's sockets, while this module
    /// has no socket and no public endpoint. Without a host endpoint, a
    /// direct-mode subscription would be created successfully and then deliver
    /// to nobody — a silent failure the user would only notice as an
    /// automation that never fires.
    fn unsupported(&self, member: &'static str) -> Error {
        Error::UnsupportedByRoute {
            route: self.name(),
            member,
        }
    }

    /// The user a connected account belongs to, from `GET /connected_accounts/{id}`.
    ///
    /// Best effort: a failed lookup or a row without an owner yields `Ok(None)`
    /// and the caller falls back to the configured entity. The lookup goes
    /// through [`Self::call`], so a rejected key counts toward the gate and
    /// stops the execute that would only be refused again; that failure (and a
    /// closed gate) is returned instead of swallowed.
    async fn account_owner(&self, connection_id: &str) -> Result<Option<String>> {
        let path = format!(
            "/connected_accounts/{}",
            segment(connection_id, "connection_id")?
        );
        match self.call(self.transport.get(&path)).await {
            Ok(value) => {
                let owner = slug_field(&value, &["user_id", "userId"]);
                tracing::debug!(
                    found = owner.is_some(),
                    "[connectors][direct] account owner lookup"
                );
                Ok(owner)
            }
            Err(error @ Error::DirectAuthGated { .. }) => Err(error),
            Err(error) if is_invalid_api_key(&error) => Err(error),
            Err(error) => {
                tracing::debug!(%error, "[connectors][direct] account owner lookup failed; using configured entity");
                Ok(None)
            }
        }
    }

    /// Refuse a trigger member unless the host receives webhooks itself.
    fn require_webhooks(&self, member: &'static str) -> Result<()> {
        if self.webhooks {
            Ok(())
        } else {
            Err(self.unsupported(member))
        }
    }

    /// Refuse a connection unless this route's entity is known to own it.
    ///
    /// Unlike execute, which falls back to the configured entity when the
    /// owner lookup fails (see [`Self::account_owner`]), a subscription fails
    /// closed: it routes the account's events to whoever holds the project
    /// webhook, and Composio checks the subscription's `user_id` against the
    /// account only on projects with 2FA enabled. A failed lookup, or an
    /// account that names no owner, is refused like another user's account.
    async fn ensure_owned(&self, connection_id: &str) -> Result<()> {
        match self.account_owner(connection_id).await? {
            Some(owner) if owner == self.entity_id => Ok(()),
            _ => Err(Error::ConnectionNotOwned {
                connection_id: connection_id.trim().to_string(),
            }),
        }
    }

    /// Create or re-enable a trigger instance as this entity.
    async fn upsert(
        &self,
        slug: &str,
        connection_id: Option<&str>,
        trigger_config: Option<serde_json::Value>,
    ) -> Result<UpsertReply> {
        if let Some(connection_id) = connection_id {
            self.ensure_owned(connection_id).await?;
        }
        // The same latest snapshot the catalog was read from.
        let mut body =
            serde_json::json!({ "user_id": self.entity_id, "toolkit_versions": "latest" });
        if let Some(connection_id) = connection_id {
            body["connected_account_id"] = connection_id.trim().into();
        }
        if let Some(config) = &trigger_config {
            body["trigger_config"] = config.clone();
        }

        let path = format!(
            "/trigger_instances/{}/upsert",
            segment(slug, "trigger slug")?
        );
        tracing::debug!(slug = %slug, "[connectors][direct] upsert trigger");
        let value = self.call(self.transport.post(&path, &body)).await?;
        let status = field(&value, &["status"]);
        let trigger_id = match slug_field(&value, &["trigger_id", "triggerId", "id"]) {
            Some(id) => id,
            // A 204 (or any reply without an id) still means the upsert took
            // effect; the instance is found among this entity's active ones.
            None => self
                .find_active_trigger(slug, connection_id, trigger_config.as_ref())
                .await?
                .ok_or(Error::Decode {
                    path,
                    message: "upsert response carried no trigger id".to_string(),
                })?,
        };
        Ok(UpsertReply { trigger_id, status })
    }

    /// The id of this entity's active instance of `slug` on `connection_id`.
    async fn find_active_trigger(
        &self,
        slug: &str,
        connection_id: Option<&str>,
        config: Option<&serde_json::Value>,
    ) -> Result<Option<String>> {
        let path = format!(
            "/trigger_instances/active?user_ids={}&show_disabled=false&limit=50",
            encode(&self.entity_id)
        );
        let rows = self.list_pages(&path).await?;
        let wanted = connection_id.map(str::trim);
        let mut matches = rows.iter().filter(|row| {
            let named = slug_field(row, &["trigger_name", "triggerName", "slug"]);
            let on_connection = field(
                row,
                &[
                    "connected_account_id",
                    "connectedAccountId",
                    "connection_id",
                ],
            );
            let same_config = config.is_none_or(|config| {
                row.get("trigger_config")
                    .or_else(|| row.get("triggerConfig"))
                    .is_some_and(|stored| stored == config)
            });
            named.is_some_and(|named| named.eq_ignore_ascii_case(slug.trim()))
                && triggers::owned_by(row, &self.entity_id)
                && wanted.is_none_or(|wanted| wanted == on_connection.as_deref().unwrap_or(""))
                && same_config
        });
        // Composio identifies the instance by slug, account and configuration.
        // Several matches mean the id cannot be told apart, and guessing one
        // could hand back another instance's id.
        let (Some(only), None) = (matches.next(), matches.next()) else {
            return Ok(None);
        };
        Ok(slug_field(only, &["id", "trigger_id", "triggerId"]))
    }

    /// Every row of a cursor-paged v3 listing.
    ///
    /// Stops at the last page. A cursor that does not advance, or a listing
    /// still going after [`MAX_PAGES`], is an
    /// [`Error::ListingIncomplete`]: a partial list would make later rows look
    /// absent.
    async fn list_pages(&self, path: &str) -> Result<Vec<serde_json::Value>> {
        let mut rows = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let page_path = match &cursor {
                Some(cursor) => format!("{path}&cursor={}", encode(cursor)),
                None => path.to_string(),
            };
            let value = self.call(self.transport.get(&page_path)).await?;
            rows.extend(triggers::rows_of(&value));
            let next = triggers::next_cursor(&value);
            if next.is_none() {
                return Ok(rows);
            }
            // A cursor that repeats would never reach the last page.
            if next == cursor {
                break;
            }
            cursor = next;
        }
        Err(Error::ListingIncomplete {
            path: path.to_string(),
            limit: MAX_PAGES,
        })
    }

    /// The auth config a v3 link names for `toolkit`.
    ///
    /// Reuses an enabled config for the toolkit, preferring a Composio-managed
    /// one; with none, creates a Composio-managed config. Every field read here
    /// is one Composio documents for `GET` / `POST /auth_configs`.
    async fn auth_config_for(&self, toolkit: &str) -> Result<String> {
        let path = format!("/auth_configs?toolkit_slug={}&limit=200", encode(toolkit));
        let rows = self
            .list_pages(&path)
            .await
            .map_err(|error| refused_request(toolkit, error))?;
        let usable: Vec<&serde_json::Value> = rows
            .iter()
            .filter(|row| {
                slug_field(row, &["status"])
                    .is_none_or(|status| status.eq_ignore_ascii_case("ENABLED"))
                    && field(row, &["toolkit"])
                        .is_some_and(|slug| slug.eq_ignore_ascii_case(toolkit))
            })
            .collect();
        let managed = |row: &&&serde_json::Value| {
            row.get("is_composio_managed")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
        };
        if let Some(id) = usable
            .iter()
            .find(managed)
            .or_else(|| usable.first())
            .and_then(|row| slug_field(row, &["id"]))
        {
            return Ok(id);
        }

        tracing::debug!(toolkit = %toolkit, "[connectors][direct] POST /auth_configs (Composio-managed)");
        let path = "/auth_configs";
        let request = serde_json::json!({
            "toolkit": { "slug": toolkit },
            "auth_config": { "type": "use_composio_managed_auth" },
        });
        let created = self
            .call(self.transport.post(path, &request))
            .await
            .map_err(|error| refused_request(toolkit, error))?;
        created
            .get("auth_config")
            .and_then(|config| slug_field(config, &["id"]))
            .ok_or(Error::Decode {
                path: path.to_string(),
                message: "auth config response carried no id".to_string(),
            })
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

/// What an upsert answered.
struct UpsertReply {
    trigger_id: String,
    status: Option<String>,
}

fn fingerprint(api_key: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    api_key.trim().hash(&mut hasher);
    hasher.finish()
}

/// Whether a failure says Composio refused the key itself: a 401 about the
/// API key. The `HTTP 401` prefix is written by our transport, not the
/// provider, so a tool's own error text ("Invalid API key for Stripe" in a
/// 400) cannot close the gate on the user's Composio key.
fn is_invalid_api_key(error: &Error) -> bool {
    matches!(error, Error::Transport { message, .. }
        if message.starts_with("HTTP 401") && message.to_ascii_lowercase().contains("api key"))
}

/// Turn a request Composio refused as invalid (HTTP 400/422) into an
/// [`Error::Authorize`] carrying its detail, so a host can show why the
/// connection could not start. Every other failure passes through unchanged.
fn refused_request(toolkit: &str, error: Error) -> Error {
    match error {
        Error::Transport { message, .. }
            if message.starts_with("HTTP 400") || message.starts_with("HTTP 422") =>
        {
            Error::Authorize {
                toolkit: toolkit.to_string(),
                message: format!("Composio rejected the connect request: {message}"),
            }
        }
        other => other,
    }
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

/// Read a slug, skipping blank strings so a later key can still supply it.
///
/// Like [`field`], but trimmed, and an empty or whitespace-only value counts as
/// absent. v3 has sent `"toolkit": ""` beside a usable `appName`.
fn slug_field(item: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| match item.get(*key) {
        Some(serde_json::Value::String(value)) => {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_string())
        }
        Some(serde_json::Value::Object(nested)) => {
            ["slug", "id", "name", "key"]
                .iter()
                .find_map(|inner| match nested.get(*inner) {
                    Some(serde_json::Value::String(value)) => {
                        let value = value.trim();
                        (!value.is_empty()).then(|| value.to_string())
                    }
                    _ => None,
                })
        }
        _ => None,
    })
}

/// Translate one v3 `connected_accounts` item into a [`ComposioConnection`].
///
/// Defensive on every field: a row missing its toolkit or status is kept with
/// empty strings rather than dropped, and `is_active` treats an empty status as
/// inactive. A malformed row therefore shows up as not-connected — which is the
/// fail-safe direction — instead of disappearing and looking deleted.
fn connection_from_v3(item: &serde_json::Value) -> Option<ComposioConnection> {
    // A blank id is no id: an empty connection id sent on to v3 makes an invalid
    // call, so the row is dropped, and a padded one is trimmed.
    let id = field(item, &["id", "nanoid", "connectedAccountId"])?
        .trim()
        .to_string();
    if id.is_empty() {
        return None;
    }
    Some(ComposioConnection {
        id,
        toolkit: slug_field(
            item,
            &[
                "toolkit",
                "appName",
                "app_name",
                "toolkit_slug",
                "appUniqueId",
            ],
        )
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
        // Composio paginates. Ask for a generous page so an ordinary tenant sees
        // its whole list in one round trip; without `limit` v3 returns its small
        // default page and connections beyond it look disconnected.
        let path = "/connected_accounts?limit=200";
        tracing::debug!("[connectors][direct] GET {path}");
        let value = self.call(self.transport.get(path)).await?;

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
        // v3 links an auth config to a user, not a toolkit to an entity: the
        // proxy-shaped body (`toolkit`, extra params, scopes) fails v3's
        // validation with a 400, so only the two fields it requires are sent.
        let auth_config_id = self.auth_config_for(toolkit).await?;
        let dropped: Vec<&str> = body
            .as_object()
            .map(|object| {
                object
                    .keys()
                    .map(String::as_str)
                    .filter(|key| *key != "toolkit")
                    .collect()
            })
            .unwrap_or_default();
        tracing::debug!(
            toolkit = %toolkit,
            auth_config_id = %auth_config_id,
            ?dropped,
            "[connectors][direct] POST /connected_accounts/link"
        );
        let request = serde_json::json!({
            "auth_config_id": auth_config_id,
            "user_id": self.entity_id,
        });

        let path = "/connected_accounts/link";
        let value = self
            .call(self.transport.post(path, &request))
            .await
            .map_err(|error| refused_request(toolkit, error))?;

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
        // `toolkit_versions=latest`: without it v3 answers from its
        // `00000000_00` snapshot, which lists no tools at all for a toolkit
        // published after it. `limit` keeps a large toolkit on one page.
        let mut query: Vec<String> = vec![
            "limit=200".to_string(),
            "toolkit_versions=latest".to_string(),
        ];
        if let Some(joined) = comma_joined(toolkits) {
            query.push(format!("toolkits={joined}"));
        }
        // v3 documents a repeated `tags` parameter ("can be specified multiple
        // times"), not the comma-joined form the proxy backend takes.
        for tag in tags
            .iter()
            .map(|tag| tag.trim())
            .filter(|tag| !tag.is_empty())
        {
            query.push(format!("tags={}", encode(tag)));
        }
        let path = format!("/tools?{}", query.join("&"));

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
        let path = format!("/tools/execute/{}", segment(tool, tool)?);
        tracing::debug!(tool = %tool, "[connectors][direct] execute");

        // Composio rejects an execute whose user does not own the connected
        // account (HTTP 400), and an account made in the dashboard belongs to a
        // generated user id rather than the configured entity. Act as the
        // account's own user when it can be read; otherwise keep the configured
        // entity.
        let entity_id = match connection_id {
            Some(connection_id) => self
                .account_owner(connection_id)
                .await?
                .unwrap_or_else(|| self.entity_id.clone()),
            None => self.entity_id.clone(),
        };
        let mut body = serde_json::json!({
            "arguments": arguments,
            "entity_id": entity_id,
        });
        if let Some(connection_id) = connection_id {
            body["connected_account_id"] = serde_json::Value::String(connection_id.to_string());
        }

        let raw = self.call(self.transport.post(&path, &body)).await?;
        Ok(execute_response_from_v3(raw))
    }

    async fn delete_connection(&self, _connection_id: &str) -> Result<ComposioDeleteResponse> {
        // Not implemented rather than impossible. Composio's v3 API almost
        // certainly exposes a delete for a connected account, but the direct
        // client this route was ported from never covered it, so there is no
        // verified request shape to port — and inventing one would ship an
        // endpoint nothing in either source repository has ever exercised.
        //
        // No behaviour is lost by refusing: the host this was extracted from
        // resolves a backend client for its delete path too, so direct mode
        // could not disconnect an account there either. When someone confirms
        // the v3 shape against a live tenant, this becomes a real call.
        Err(self.unsupported("DeleteConnection"))
    }

    async fn list_github_repos(
        &self,
        _connection_id: Option<&str>,
    ) -> Result<ComposioGithubReposResponse> {
        // Only ever used to pick a repository for a GitHub-scoped trigger. With
        // webhooks off there are no triggers to pick for; with them on, v3
        // takes the repository as ordinary required trigger configuration, and
        // there is no verified v3 request shape for the listing to port.
        Err(self.unsupported("ListGithubRepos"))
    }

    async fn list_available_triggers(
        &self,
        toolkit: &str,
        _connection_id: Option<&str>,
    ) -> Result<ComposioAvailableTriggersResponse> {
        // The connection only matters to the proxy's per-repository GitHub
        // fan-out. v3 lists a trigger type once per toolkit, so every row here
        // is static and a repository is ordinary required configuration.
        self.require_webhooks("ListAvailableTriggers")?;
        // `toolkit_versions=latest`, as for tools: the default is a pinned
        // snapshot that can lack newer triggers and their config schemas.
        let path = format!(
            "/triggers_types?toolkit_slugs={}&limit=50&toolkit_versions=latest",
            encode(toolkit)
        );
        tracing::debug!(toolkit = %toolkit, "[connectors][direct] list_available_triggers");
        let rows = self.list_pages(&path).await?;
        Ok(ComposioAvailableTriggersResponse {
            triggers: rows
                .iter()
                .filter_map(triggers::available_trigger_from_v3)
                .collect(),
        })
    }

    async fn list_triggers(&self, toolkit: Option<&str>) -> Result<ComposioActiveTriggersResponse> {
        // Always filtered to this entity: one Composio key can hold many users,
        // and a listing without `user_ids` would show every one of them.
        self.require_webhooks("ListTriggers")?;
        let path = format!(
            "/trigger_instances/active?user_ids={}&show_disabled=false&limit=50",
            encode(&self.entity_id)
        );
        tracing::debug!("[connectors][direct] list_triggers");
        let rows: Vec<_> = self
            .list_pages(&path)
            .await?
            .into_iter()
            .filter(|row| triggers::owned_by(row, &self.entity_id))
            .collect();

        // A row seldom names its toolkit, and a toolkit slug can hold an
        // underscore, so an unfiltered listing asks the connections which
        // toolkit each row's account is. A failed lookup only costs accuracy:
        // those rows fall back to the slug's first word.
        let wants_lookup = toolkit.is_none_or(|t| t.trim().is_empty())
            && rows.iter().any(triggers::names_no_toolkit);
        let connection_toolkits: std::collections::HashMap<String, String> = if wants_lookup {
            // Paged, unlike `list_connections`: an account on a later page
            // would otherwise miss the map and fall back to the first word.
            match self.list_pages("/connected_accounts?limit=200").await {
                Ok(accounts) => accounts
                    .iter()
                    .filter_map(connection_from_v3)
                    .map(|connection| (connection.id, connection.toolkit.to_ascii_lowercase()))
                    .collect(),
                Err(error) => {
                    tracing::debug!(%error, "[connectors][direct] toolkit lookup failed");
                    std::collections::HashMap::default()
                }
            }
        } else {
            std::collections::HashMap::default()
        };

        Ok(ComposioActiveTriggersResponse {
            triggers: rows
                .iter()
                .filter_map(|row| {
                    triggers::active_trigger_from_v3(row, toolkit, &connection_toolkits)
                })
                .collect(),
        })
    }

    async fn create_trigger(
        &self,
        slug: &str,
        connection_id: Option<&str>,
        trigger_config: Option<serde_json::Value>,
    ) -> Result<ComposioCreateTriggerResponse> {
        self.require_webhooks("CreateTrigger")?;
        let reply = self.upsert(slug, connection_id, trigger_config).await?;
        Ok(ComposioCreateTriggerResponse {
            trigger_id: reply.trigger_id,
            status: reply.status,
        })
    }

    async fn enable_trigger(
        &self,
        connection_id: &str,
        slug: &str,
        trigger_config: Option<serde_json::Value>,
    ) -> Result<ComposioEnableTriggerResponse> {
        // The upsert re-enables a matching disabled instance, so enabling and
        // creating are the same call — as they are on the proxy route.
        self.require_webhooks("EnableTrigger")?;
        let reply = self
            .upsert(slug, Some(connection_id), trigger_config)
            .await?;
        Ok(ComposioEnableTriggerResponse {
            trigger_id: reply.trigger_id,
            slug: slug.trim().to_string(),
            connection_id: connection_id.trim().to_string(),
        })
    }

    async fn disable_trigger(&self, trigger_id: &str) -> Result<ComposioDisableTriggerResponse> {
        // Disabling removes the subscription, as the proxy route's DELETE does.
        // The instance is looked up as this entity first: its id alone would
        // let one user of a shared key remove another's subscription.
        self.require_webhooks("DisableTrigger")?;
        let trigger_id = trigger_id.trim();
        let lookup = format!(
            "/trigger_instances/active?trigger_ids={}&user_ids={}&show_disabled=true&limit=50",
            encode(trigger_id),
            encode(&self.entity_id)
        );
        tracing::debug!(trigger_id = %trigger_id, "[connectors][direct] disable_trigger");
        let found = self.call(self.transport.get(&lookup)).await?;
        let owned = triggers::rows_of(&found).iter().any(|row| {
            slug_field(row, &["id", "trigger_id", "triggerId"]).as_deref() == Some(trigger_id)
                && triggers::names_owner(row, &self.entity_id)
        });
        if !owned {
            return Err(Error::TriggerNotOwned {
                trigger_id: trigger_id.to_string(),
            });
        }

        let path = format!(
            "/trigger_instances/manage/{}",
            segment(trigger_id, "trigger_id")?
        );
        self.call(self.transport.delete(&path)).await?;
        Ok(ComposioDisableTriggerResponse { deleted: true })
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
            // A row with a blank slug cannot be called, and offering it to a
            // model as a function with no name is worse than not listing it.
            if name.is_empty() {
                return None;
            }
            Some(crate::ComposioToolSchema {
                kind: "function".to_string(),
                function: crate::ComposioToolFunction {
                    name,
                    // v3 rows without a description are described by their name.
                    description: field(row, &["description", "name"]),
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
/// response carries no verdict and no `error` at all: v3 answers a plain result for some
/// actions, and defaulting to failure would report a completed action as
/// broken. And `cost_usd` is zero because direct mode carries no billing
/// margin — the user is paying Composio, not us.
fn execute_response_from_v3(raw: serde_json::Value) -> ComposioExecuteResponse {
    // An `error` with no verdict is a failure, not the plain-result default.
    let has_error = raw.get("error").is_some_and(|error| !error.is_null());
    let successful = raw
        .get("successful")
        .or_else(|| raw.get("success"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(!has_error);
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
#[path = "direct_tests.rs"]
mod test;

#[cfg(test)]
#[path = "direct_triggers_tests.rs"]
mod triggers_test;
