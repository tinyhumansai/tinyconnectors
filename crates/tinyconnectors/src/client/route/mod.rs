//! The two ways to reach Composio, behind one trait.
//!
//! Composio can be reached two ways, and they are not the same API:
//!
//! - **proxy** — through the `TinyHumans` backend at
//!   `/agent-integrations/composio/*`. The backend owns the Composio API key,
//!   the billing margin, the per-user toolkit allowlist, and the HMAC
//!   verification of inbound webhooks.
//! - **direct** — straight at `backend.composio.dev/api/v3` with a
//!   user-supplied `x-api-key`. No allowlist, no margin, no webhook fan-out.
//!
//! They differ in base URL, in authentication header, in paths, *and in
//! response shape*. A route is therefore not a transport: it owns the paths it
//! calls and the translation of what comes back into this crate's envelopes, so
//! that [`crate::client::ComposioClient`] — and everything above it — never
//! branches on mode.
//!
//! # Choosing one is the host's job
//!
//! The module implements both and decides neither. Which route to use depends
//! on whether the user is signed in, whether they supplied their own key, and
//! which the product prefers — all host policy, and all upstream of this crate.
//! The host states its choice in the module configuration blob.
//!
//! # The routes are not equivalent
//!
//! Direct mode genuinely cannot answer several members, and says so with
//! [`crate::Error::UnsupportedByRoute`] naming the route and the member:
//!
//! - `ListToolkits` — there is no per-user allowlist when you talk to Composio
//!   directly; you see the whole catalog. An empty list would read as "you may
//!   connect nothing".
//! - `DeleteConnection` — the proxy's version also clears memory sourced from
//!   the connection, which Composio knows nothing about. A bare delete would
//!   silently orphan the user's synced content.
//! - **Every trigger member, and the GitHub repository listing.** Triggers are
//!   webhooks, and a webhook has to arrive somewhere. The proxy backend
//!   HMAC-verifies deliveries and fans them out over the user's sockets; the
//!   module has no socket and no public endpoint. A direct-mode subscription
//!   would be created successfully and then deliver to nobody, which is worse
//!   than not offering it.

mod direct;
mod proxy;

pub use direct::{COMPOSIO_API_BASE, DirectRoute, INVALID_API_KEY_THRESHOLD};
pub use proxy::ProxyRoute;

use async_trait::async_trait;

use crate::{
    ComposioActiveTriggersResponse, ComposioAuthorizeResponse, ComposioAvailableTriggersResponse,
    ComposioConnectionsResponse, ComposioCreateTriggerResponse, ComposioDeleteResponse,
    ComposioDisableTriggerResponse, ComposioEnableTriggerResponse, ComposioExecuteResponse,
    ComposioGithubReposResponse, ComposioToolkitsResponse, ComposioToolsResponse, Result,
};

/// One way of reaching Composio.
///
/// Implementors own their paths, their authentication, and the translation of
/// upstream responses into this crate's envelopes.
#[async_trait]
pub trait Route: Send + Sync + std::fmt::Debug {
    /// `"proxy"` or `"direct"` — for diagnostics, and for naming the route in a
    /// [`crate::Error::UnsupportedByRoute`].
    fn name(&self) -> &'static str;

    /// The toolkits this user may connect.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::UnsupportedByRoute`] on a route with no
    /// allowlist concept, and otherwise the underlying failure.
    async fn list_toolkits(&self) -> Result<ComposioToolkitsResponse>;

    /// Every connection for this user, active or not.
    ///
    /// # Errors
    ///
    /// Returns the underlying transport or decode failure.
    async fn list_connections(&self) -> Result<ComposioConnectionsResponse>;

    /// Begin an OAuth handoff for `toolkit`.
    ///
    /// `body` is the request the client assembled — already validated, with any
    /// required scopes merged in. A route sends what its own API accepts.
    ///
    /// # Errors
    ///
    /// Returns the underlying transport or decode failure.
    async fn authorize(
        &self,
        toolkit: &str,
        body: &serde_json::Value,
    ) -> Result<ComposioAuthorizeResponse>;

    /// List the callable tools for `toolkits`, optionally narrowed by `tags`.
    ///
    /// Tags are OR semantics: more tags broaden the result.
    ///
    /// # Errors
    ///
    /// Returns the underlying transport or decode failure.
    async fn list_tools(
        &self,
        toolkits: &[String],
        tags: &[String],
    ) -> Result<ComposioToolsResponse>;

    /// Run one action.
    ///
    /// `arguments` is already prepared — normalized and validated by
    /// [`crate::execute`]. A route sends it as its own API expects.
    ///
    /// A provider that answers `successful: false` is not an error: the call
    /// got a real answer. Only a call that never completed is an `Err`.
    ///
    /// # Errors
    ///
    /// Returns the underlying transport or decode failure.
    async fn execute(
        &self,
        tool: &str,
        arguments: &serde_json::Value,
        connection_id: Option<&str>,
    ) -> Result<ComposioExecuteResponse>;

    /// Remove a connection.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::UnsupportedByRoute`] on a route that does not
    /// offer it, and otherwise the underlying failure.
    async fn delete_connection(&self, connection_id: &str) -> Result<ComposioDeleteResponse>;

    /// List the repositories a connected GitHub account can see.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::UnsupportedByRoute`] on a route that does not
    /// offer it, and otherwise the underlying failure.
    async fn list_github_repos(
        &self,
        connection_id: Option<&str>,
    ) -> Result<ComposioGithubReposResponse>;

    /// List the triggers `toolkit` offers.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::UnsupportedByRoute`] on a route that does not
    /// offer it, and otherwise the underlying failure.
    async fn list_available_triggers(
        &self,
        toolkit: &str,
        connection_id: Option<&str>,
    ) -> Result<ComposioAvailableTriggersResponse>;

    /// List the caller's enabled trigger subscriptions.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::UnsupportedByRoute`] on a route that does not
    /// offer it, and otherwise the underlying failure.
    async fn list_triggers(
        &self,
        toolkit: Option<&str>,
    ) -> Result<ComposioActiveTriggersResponse>;

    /// Create a trigger subscription.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::UnsupportedByRoute`] on a route that does not
    /// offer it, and otherwise the underlying failure.
    async fn create_trigger(
        &self,
        slug: &str,
        connection_id: Option<&str>,
        trigger_config: Option<serde_json::Value>,
    ) -> Result<ComposioCreateTriggerResponse>;

    /// Enable a trigger subscription.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::UnsupportedByRoute`] on a route that does not
    /// offer it, and otherwise the underlying failure.
    async fn enable_trigger(
        &self,
        connection_id: &str,
        slug: &str,
        trigger_config: Option<serde_json::Value>,
    ) -> Result<ComposioEnableTriggerResponse>;

    /// Disable a trigger subscription.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::UnsupportedByRoute`] on a route that does not
    /// offer it, and otherwise the underlying failure.
    async fn disable_trigger(&self, trigger_id: &str) -> Result<ComposioDisableTriggerResponse>;
}
