//! The two ways to reach Composio, behind one trait.
//!
//! Composio can be reached two ways, and they are not the same API:
//!
//! - **proxy** — through the TinyHumans backend at
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
//! Direct mode genuinely cannot answer some members. There is no per-user
//! toolkit allowlist when you talk to Composio directly — you see the whole
//! catalog — so `ListToolkits` has nothing to return. Those members answer with
//! [`crate::Error::UnsupportedByRoute`] naming the route and the member, rather
//! than an empty list a caller would read as "you may connect nothing".

mod direct;
mod proxy;

pub use direct::DirectRoute;
pub use proxy::ProxyRoute;

use async_trait::async_trait;

use crate::{
    ComposioAuthorizeResponse, ComposioConnectionsResponse, ComposioDeleteResponse,
    ComposioToolkitsResponse, Result,
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

    /// Remove a connection.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::UnsupportedByRoute`] on a route that does not
    /// offer it, and otherwise the underlying failure.
    async fn delete_connection(&self, connection_id: &str) -> Result<ComposioDeleteResponse>;
}
