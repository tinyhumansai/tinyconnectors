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
//! the environment, does not hold a Composio key, and does not authenticate a
//! user itself. It is given a backend URL and a token for the already
//! signed-in user, and it can only reach that backend.
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
    ComposioDeleteConnectionRequest, ComposioDeleteResponse, ComposioToolkitsResponse, names,
};

use crate::client::{ComposioClient, HttpTransport};

/// Configuration the host hands the module at load time.
///
/// Reachable only through `module_export!`, which names the type in the ABI
/// entrypoint it generates; nothing re-exports it from the crate root.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ModuleConfig {
    /// Base URL of the connector backend, e.g. `https://api.example.com`.
    pub(crate) base_url: String,
    /// Bearer token for the signed-in user.
    ///
    /// Never logged and never returned through a member — see
    /// `HttpTransport`'s hand-written `Debug`.
    pub(crate) auth_token: String,
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
}

/// Flatten a crate error onto the bus.
fn to_bus_error(error: &crate::Error) -> tinybus::Error {
    tinybus::Error::failed(error.to_string())
}

async fn setup(connection: Connection, config: ModuleConfig) -> TinyBusResult<()> {
    let transport = Arc::new(HttpTransport::new(config.base_url, config.auth_token));
    let service = ConnectorService {
        client: ComposioClient::new(transport),
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
    methods = ["ListToolkits", "ListConnections", "Authorize", "DeleteConnection"],
    signals = [],
    requires = [],
    optional = [],
    lazy = false,
}

#[cfg(test)]
mod test;
