//! `TinyBus` module entrypoint and bus-facing interface.
//!
//! This adapter keeps the connector implementation independent of `TinyBus`
//! while exposing it as an installable, dynamically loaded integration. The
//! names and payload types it serves come from [`tinyconnectors_bus`], so a
//! host spells them from the contract crate instead of repeating string
//! literals.
//!
//! # Where the credential comes from
//!
//! The host supplies [`ModuleConfig`] as the module's JSON configuration blob
//! at load time. That is deliberately the only way in: the module does not read
//! the environment and does not authenticate a user itself.
//!
//! # Which route, and who decides
//!
//! The blob also says *how* to reach Composio — proxied through the `TinyHumans`
//! backend, or directly with a user-supplied API key. The module implements
//! both and chooses neither: whether the user is signed in, whether they
//! supplied a key, and which the product prefers are all host policy, and all
//! upstream of this crate.
//!
//! The routes are not equivalent, and the module says so rather than pretending
//! otherwise — see [`crate::client::route`]. A member the live route cannot
//! answer fails with [`crate::Error::UnsupportedByRoute`] naming both.
//!
//! # Why failures cross as messages, not variants
//!
//! `TinyBus` carries an error name and a message, so [`crate::Error`]'s
//! structure is flattened at this boundary. The rendered message keeps the
//! distinguishing detail — the failing path, or the user-facing rate-limit
//! guidance — because that is all a host will have to act on.

use std::sync::Arc;

use serde::Deserialize;
use tinybus::{Connection, Result as TinyBusResult};
use tinyconnectors_bus::{
    ComposioAuthorizeRequest, ComposioAuthorizeResponse, ComposioConnectionsResponse,
    ComposioDeleteConnectionRequest, ComposioDeleteResponse, ComposioExecuteRequest,
    ComposioExecuteResponse, ComposioListToolsRequest, ComposioToolkitsResponse,
    ComposioToolsResponse, names,
};

use crate::client::{
    COMPOSIO_API_BASE, ComposioClient, DirectRoute, HttpTransport, ProxyRoute, Route,
};

/// Configuration the host hands the module at load time.
///
/// Tagged by `route`, so the two variants cannot be confused and a blob missing
/// the credential its route needs fails at load rather than producing a module
/// that answers every member with a 401.
///
/// ```json
/// { "route": "proxy",  "base_url": "https://api.example.com", "auth_token": "…" }
/// { "route": "direct", "api_key": "…", "entity_id": "default" }
/// ```
///
/// Reachable only through `module_export!`, which names the type in the ABI
/// entrypoint it generates; nothing re-exports it from the crate root.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "route", rename_all = "snake_case")]
pub(crate) enum ModuleConfig {
    /// Reach Composio through the `TinyHumans` backend.
    Proxy {
        /// Base URL of the connector backend, e.g. `https://api.example.com`.
        base_url: String,
        /// Bearer token for the signed-in user.
        ///
        /// Never logged and never returned through a member — see
        /// `HttpTransport`'s hand-written `Debug`.
        auth_token: String,
    },
    /// Reach Composio directly with a user-supplied key.
    Direct {
        /// The user's own Composio API key, sent as `x-api-key`.
        api_key: String,
        /// Composio entity the connections belong to. Defaults to `"default"`,
        /// which is what Composio assumes when none is given.
        #[serde(default)]
        entity_id: Option<String>,
        /// Override for Composio's API base. Present for a loopback test
        /// server; production leaves it out and gets [`COMPOSIO_API_BASE`].
        #[serde(default)]
        base_url: Option<String>,
    },
}

impl ModuleConfig {
    /// Build the route this configuration selects.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::InsecureBaseUrl`] if the configured base URL
    /// would send the credential anywhere other than HTTPS or a loopback
    /// address.
    fn into_route(self) -> crate::Result<Arc<dyn Route>> {
        match self {
            Self::Proxy {
                base_url,
                auth_token,
            } => {
                let transport = Arc::new(HttpTransport::bearer(&base_url, auth_token)?);
                Ok(Arc::new(ProxyRoute::new(transport)))
            }
            Self::Direct {
                api_key,
                entity_id,
                base_url,
            } => {
                let base_url = base_url.unwrap_or_else(|| COMPOSIO_API_BASE.to_string());
                let transport = Arc::new(HttpTransport::api_key(&base_url, api_key.clone())?);
                Ok(Arc::new(DirectRoute::new(
                    transport,
                    &api_key,
                    entity_id.unwrap_or_default(),
                )))
            }
        }
    }
}

struct ConnectorService {
    client: ComposioClient,
}

#[tinybus::interface(name = "ai.tinyhumans.connectors.Composio")]
impl ConnectorService {
    async fn list_toolkits(&self) -> TinyBusResult<ComposioToolkitsResponse> {
        self.client
            .list_toolkits()
            .await
            .map_err(|error| to_bus_error(&error))
    }

    async fn list_connections(&self) -> TinyBusResult<ComposioConnectionsResponse> {
        self.client
            .list_connections()
            .await
            .map_err(|error| to_bus_error(&error))
    }

    async fn authorize(
        &self,
        request: ComposioAuthorizeRequest,
    ) -> TinyBusResult<ComposioAuthorizeResponse> {
        let toolkit = request.toolkit.clone();
        let result = crate::oauth::authorize_with_rate_limit_retry(|| {
            self.client
                .authorize(&request.toolkit, request.extra_params.clone())
        })
        .await;

        result
            .map_err(|error| crate::oauth::wrap_authorize_rate_limit_error(&toolkit, error))
            .map_err(|error| to_bus_error(&error))
    }

    async fn delete_connection(
        &self,
        request: ComposioDeleteConnectionRequest,
    ) -> TinyBusResult<ComposioDeleteResponse> {
        self.client
            .delete_connection(&request.connection_id)
            .await
            .map_err(|error| to_bus_error(&error))
    }

    async fn list_tools(
        &self,
        request: ComposioListToolsRequest,
    ) -> TinyBusResult<ComposioToolsResponse> {
        self.client
            .list_tools(&request.toolkits, &request.tags)
            .await
            .map_err(|error| to_bus_error(&error))
    }

    async fn execute(
        &self,
        request: ComposioExecuteRequest,
    ) -> TinyBusResult<ComposioExecuteResponse> {
        self.client
            .execute(
                &request.tool,
                request.arguments,
                request.connection_id.as_deref(),
            )
            .await
            .map_err(|error| to_bus_error(&error))
    }
}

/// Flatten a crate error onto the bus.
fn to_bus_error(error: &crate::Error) -> tinybus::Error {
    tinybus::Error::failed(error.to_string())
}

async fn setup(connection: Connection, config: ModuleConfig) -> TinyBusResult<()> {
    let route = config.into_route().map_err(|error| to_bus_error(&error))?;
    tracing::info!(
        route = route.name(),
        "[connectors] serving connector surface"
    );
    let service = ConnectorService {
        client: ComposioClient::new(route),
    };

    connection
        .serve_at(names::OBJECT_PATH.try_into()?, service)
        .await?;
    connection.request_name(names::INTERFACE).await?;
    Ok(())
}

tinybus_module::module_export! {
    setup = setup,
    config = ModuleConfig,
    worker_threads = 1,
    provides = ["ai.tinyhumans.connectors.Composio"],
    methods = [
        "ListToolkits",
        "ListConnections",
        "Authorize",
        "DeleteConnection",
        "ListTools",
        "Execute",
    ],
    signals = [],
    requires = [],
    optional = [],
    lazy = false,
}

#[cfg(test)]
mod test;
