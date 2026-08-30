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

use std::sync::{Arc, RwLock};

use serde::Deserialize;
use tinybus::{Connection, Result as TinyBusResult};
use tinyconnectors_bus::{
    ComposioActiveTriggersResponse, ComposioAgentReadyToolkitsResponse, ComposioAuthorizeRequest,
    ComposioAuthorizeResponse, ComposioAvailableTriggersResponse, ComposioCapabilitiesResponse,
    ComposioConfigureRequest, ComposioConfigureResponse, ComposioConnectionsResponse,
    ComposioCreateTriggerRequest, ComposioCreateTriggerResponse, ComposioDeleteConnectionRequest,
    ComposioDeleteResponse, ComposioDisableTriggerRequest, ComposioDisableTriggerResponse,
    ComposioEnableTriggerRequest, ComposioEnableTriggerResponse, ComposioExecuteRequest,
    ComposioExecuteResponse, ComposioGetUserScopesRequest, ComposioGithubReposResponse,
    ComposioIdentityFailure, ComposioListAvailableTriggersRequest, ComposioListGithubReposRequest,
    ComposioListToolsRequest, ComposioListTriggerHistoryRequest, ComposioListTriggersRequest,
    ComposioRefreshIdentitiesResponse, ComposioSetUserScopesRequest, ComposioToolkitsResponse,
    ComposioToolsResponse, ComposioTriggerHistoryResult, ComposioUserProfile,
    ComposioUserProfileRequest, ComposioUserScopes, ComposioUserScopesResponse,
    ConnectorSyncRequest, ConnectorSyncResponse, names,
};

use crate::client::{
    COMPOSIO_API_BASE, ComposioClient, DirectRoute, HttpTransport, ProxyRoute, Route,
};
use tinyconnectors_sync::{
    ProviderContext, ProviderRegistry, SyncLimits, SyncReason, SyncStateStore, UserScopePref,
    classify_unknown, find_curated, run_sync,
};

use crate::providers::ClientActions;
use crate::state::FileStateStore;
use crate::triggers::TriggerArchive;

/// How to reach Composio, when the host has said.
///
/// Tagged by `route`, so the two variants cannot be confused and a blob naming
/// a route without that route's credential is refused rather than producing a
/// client that answers every call with a 401.
///
/// ```json
/// { "route": "proxy",  "base_url": "https://api.example.com", "auth_token": "…",
///   "state_dir": "/var/lib/openhuman" }
/// { "route": "direct", "api_key": "…", "entity_id": "default" }
/// ```
///
/// Reachable only through `module_export!`, which names the type in the ABI
/// entrypoint it generates; nothing re-exports it from the crate root.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "route", rename_all = "snake_case")]
pub(crate) enum RouteConfig {
    /// Reach Composio through the `TinyHumans` backend.
    Proxy {
        /// Directory the module may keep state in, for the trigger archive.
        ///
        /// Optional: a host that never enables triggers has no reason to hand
        /// the module a writable directory, and requiring one would make every
        /// deployment carry a path it does not use.
        #[serde(default)]
        state_dir: Option<std::path::PathBuf>,
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
        /// Directory the module may keep state in. See the proxy variant —
        /// though the direct route serves no trigger members, so this is
        /// accepted for symmetry and goes unused.
        #[serde(default)]
        state_dir: Option<std::path::PathBuf>,
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

/// Configuration the host hands the module at load time.
///
/// Every field is optional, and an empty blob is valid. That is deliberate: a
/// module that refuses to load without a credential cannot be loaded by a host
/// that discovers modules generically, and — more importantly — cannot answer
/// the members that need no credential at all. `ListCapabilities` describes the
/// compiled build, and a signed-out user deciding what to connect is exactly
/// who needs it.
///
/// A member that does need a route says so when it is called, naming what is
/// missing. That is a worse error than a load failure only if you assume every
/// caller wanted a route; most callers of the capability members did not.
#[derive(Debug, Clone, Default)]
pub(crate) struct ModuleConfig {
    /// How to reach Composio, when the host has said.
    route: Option<RouteConfig>,
}

impl<'de> Deserialize<'de> for ModuleConfig {
    /// Absent `route` means unconfigured; present but malformed is an error.
    ///
    /// Hand-written because the obvious `#[serde(flatten)] Option<RouteConfig>`
    /// gets this exactly wrong: it turns a *malformed* route into `None`, so a
    /// blob naming `"proxy"` with a misspelled `auth_token` would load as an
    /// unconfigured module and silently answer every connector member with
    /// "no route configured". A typo would disable connectors and look like a
    /// deliberate choice.
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        if value.get("route").is_none_or(serde_json::Value::is_null) {
            return Ok(Self { route: None });
        }
        RouteConfig::deserialize(value)
            .map(|route| Self { route: Some(route) })
            .map_err(serde::de::Error::custom)
    }
}

impl ModuleConfig {
    /// The directory the module may keep state in, if the host named one.
    fn state_dir(&self) -> Option<&std::path::Path> {
        self.route.as_ref().and_then(RouteConfig::state_dir)
    }

    /// Build the route this configuration selects, if it selects one.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::InsecureBaseUrl`] if the configured base URL
    /// would send the credential somewhere it must not go.
    fn into_route(self) -> crate::Result<Option<Arc<dyn Route>>> {
        self.route.map(RouteConfig::into_route).transpose()
    }
}

impl RouteConfig {
    /// Reuse the load-time route builder for a runtime reconfiguration.
    ///
    /// The two carry the same description of a route, so building one from the
    /// other keeps a single place that turns a description into a transport —
    /// including the check that refuses to send a credential over plain HTTP to
    /// anywhere but a loopback address.
    ///
    /// `state_dir` is dropped rather than carried: the trigger archive is
    /// opened once at load, and letting a later call move it would leave
    /// already-archived history behind with nothing pointing at it.
    fn try_from(request: ComposioConfigureRequest) -> Option<Self> {
        match request {
            ComposioConfigureRequest::None => None,
            ComposioConfigureRequest::Proxy {
                base_url,
                auth_token,
            } => Some(Self::Proxy {
                base_url,
                auth_token,
                state_dir: None,
            }),
            ComposioConfigureRequest::Direct {
                api_key,
                entity_id,
                base_url,
            } => Some(Self::Direct {
                api_key,
                entity_id,
                base_url,
                state_dir: None,
            }),
        }
    }
}

impl RouteConfig {
    /// The directory the module may keep state in, if the host named one.
    fn state_dir(&self) -> Option<&std::path::Path> {
        match self {
            Self::Proxy { state_dir, .. } | Self::Direct { state_dir, .. } => state_dir.as_deref(),
        }
    }

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
                ..
            } => {
                let transport = Arc::new(HttpTransport::bearer(&base_url, auth_token)?);
                Ok(Arc::new(ProxyRoute::new(transport)))
            }
            Self::Direct {
                api_key,
                entity_id,
                base_url,
                ..
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
    /// The Composio client, when a route has been configured.
    ///
    /// `None` is a module loaded with no configuration — allowed, so the
    /// capability members can answer. Everything that talks to Composio goes
    /// through [`ConnectorService::client`], which explains what is missing
    /// rather than failing obscurely.
    ///
    /// Swappable, because a host's credential does not stand still. A user
    /// signs in, supplies an API key, or switches mode long after this module
    /// was lazily loaded, and a route fixed at load time would leave them
    /// unable to reach Composio until the application restarted. `Configure`
    /// replaces it in place.
    client: Arc<RwLock<Option<ComposioClient>>>,
    /// The archive of webhook deliveries, when the host gave the module a
    /// directory to keep state in.
    ///
    /// Optional because a host that never enables triggers has no reason to
    /// hand the module a writable directory. Asking for one unconditionally
    /// would make every deployment carry a path it does not use.
    archive: Option<TriggerArchive>,
    /// The toolkits this build knows how to read.
    ///
    /// Answers the capability members without touching the network, and gives
    /// the profile members the action slug and identity field for a toolkit.
    registry: ProviderRegistry,
    /// How providers run their actions.
    actions: Arc<ClientActions>,
    /// Where providers persist cursors and budgets.
    state: Arc<dyn SyncStateStore>,
}

impl ConnectorService {
    /// The client, or an error naming what the host did not configure.
    fn client(&self) -> TinyBusResult<ComposioClient> {
        self.client
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .ok_or_else(|| {
                tinybus::Error::failed(
                    "this module was loaded without a connector route: pass a `route` of \
                     \"proxy\" or \"direct\" in its configuration, or call Configure, to \
                     reach Composio",
                )
            })
    }

    /// Assemble the context one provider needs for one call.
    fn context(&self, toolkit: &str, connection_id: &str) -> ProviderContext {
        ProviderContext {
            toolkit: toolkit.to_string(),
            connection_id: connection_id.to_string(),
            // A profile read produces no records, so the source it would write
            // to is not consulted. Named after the connection anyway so a log
            // line ties the call to something.
            source_id: format!("{toolkit}:{connection_id}"),
            limits: SyncLimits::default(),
            actions: self.actions.clone(),
            state: self.state.clone(),
        }
    }

    /// Read one connection's identity through its toolkit's provider.
    async fn profile_of(
        &self,
        toolkit: &str,
        connection_id: &str,
    ) -> TinyBusResult<ComposioUserProfile> {
        let provider = self.registry.get(toolkit).ok_or_else(|| {
            tinybus::Error::failed(format!(
                "no provider for toolkit `{toolkit}`: this build does not know how to read its \
                 profile"
            ))
        })?;

        let profile = provider
            .fetch_user_profile(&self.context(toolkit, connection_id))
            .await
            .map_err(|error| tinybus::Error::failed(error.to_string()))?;

        Ok(ComposioUserProfile {
            toolkit: profile.toolkit,
            connection_id: profile.connection_id,
            display_name: profile.display_name,
            email: profile.email,
            username: profile.username,
            avatar_url: profile.avatar_url,
            profile_url: profile.profile_url,
            extras: profile.extras,
        })
    }

    /// Whether the user's preference permits `action`.
    ///
    /// The scope comes from the toolkit's curated catalog when it lists the
    /// action, and from the verb heuristic otherwise — a toolkit nobody has
    /// curated still has to obey the preference, or "read only" would mean
    /// nothing for exactly the integrations least understood.
    ///
    /// An action whose toolkit cannot be derived is permitted: it is a slug
    /// shaped unlike anything Composio publishes, and refusing on that basis
    /// would break a working action over a naming convention.
    async fn permits(&self, action: &str) -> TinyBusResult<bool> {
        let Some(toolkit) = tinyconnectors_sync::toolkit_from_slug(action) else {
            return Ok(true);
        };
        let pref = UserScopePref::load(self.state.as_ref(), &toolkit)
            .await
            .map_err(|error| tinybus::Error::failed(error.to_string()))?;

        let scope = self
            .registry
            .get(&toolkit)
            .and_then(|provider| {
                provider
                    .curated_tools()
                    .and_then(|catalog| find_curated(catalog, action).map(|tool| tool.scope))
            })
            .unwrap_or_else(|| classify_unknown(action));

        Ok(pref.allows(scope))
    }

    /// The first active connection for `toolkit`.
    async fn first_active_connection(&self, toolkit: &str) -> TinyBusResult<String> {
        let wanted = toolkit.trim().to_ascii_lowercase();
        self.client()?
            .list_connections()
            .await
            .map_err(|error| to_bus_error(&error))?
            .connections
            .into_iter()
            .find(|connection| connection.is_active() && connection.normalized_toolkit() == wanted)
            .map(|connection| connection.id)
            .ok_or_else(|| {
                tinybus::Error::failed(format!("no active connection for toolkit `{toolkit}`"))
            })
    }
}

#[tinybus::interface(name = "ai.tinyhumans.connectors.Composio")]
impl ConnectorService {
    /// Install or replace the route this module reaches Composio over.
    ///
    /// A host supplies a route at load time, but its credential does not stand
    /// still. Under a lazy load policy the module is commonly up *before* the
    /// user signs in, and a route fixed at load would leave them unable to
    /// reach Composio until the application restarted. Sign-out is the same
    /// problem pointed the other way: a stale bearer answers 401 to everything.
    ///
    /// Replacing is deliberately unconditional. The host owns the decision of
    /// which route to use — this module implements both and chooses neither —
    /// so a `Configure` is an instruction, not a proposal. That includes
    /// [`ComposioConfigureRequest::None`], which drops the credential rather
    /// than keeping one whose session has ended.
    // `async` with nothing awaited: swapping a route is a lock and a move, but
    // every member of a `#[tinybus::interface]` impl has to be async to be
    // dispatched. Narrow, and on this one member only.
    #[allow(
        clippy::unused_async,
        clippy::unused_async_trait_impl,
        reason = "required by the interface dispatcher"
    )]
    async fn configure(
        &self,
        request: ComposioConfigureRequest,
    ) -> TinyBusResult<ComposioConfigureResponse> {
        let (client, name) = match RouteConfig::try_from(request) {
            Some(config) => {
                let route = config.into_route().map_err(|error| to_bus_error(&error))?;
                let name = route.name().to_string();
                (Some(ComposioClient::new(route)), name)
            }
            // Not a failure: the host is telling us its user signed out. The
            // members that need a credential go back to saying so, which is
            // what a signed-out user should be told.
            None => (None, "none".to_string()),
        };
        tracing::info!(route = %name, "[connectors] route reconfigured");
        *self
            .client
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = client;
        Ok(ComposioConfigureResponse { route: name })
    }

    async fn list_toolkits(&self) -> TinyBusResult<ComposioToolkitsResponse> {
        self.client()?
            .list_toolkits()
            .await
            .map_err(|error| to_bus_error(&error))
    }

    async fn list_connections(&self) -> TinyBusResult<ComposioConnectionsResponse> {
        self.client()?
            .list_connections()
            .await
            .map_err(|error| to_bus_error(&error))
    }

    async fn authorize(
        &self,
        request: ComposioAuthorizeRequest,
    ) -> TinyBusResult<ComposioAuthorizeResponse> {
        let toolkit = request.toolkit.clone();
        // Resolved once, outside the retry: a missing route is not something a
        // second attempt fixes.
        let client = self.client()?;
        let result = crate::oauth::authorize_with_rate_limit_retry(|| {
            client.authorize(&request.toolkit, request.extra_params.clone())
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
        self.client()?
            .delete_connection(&request.connection_id)
            .await
            .map_err(|error| to_bus_error(&error))
    }

    async fn list_tools(
        &self,
        request: ComposioListToolsRequest,
    ) -> TinyBusResult<ComposioToolsResponse> {
        let mut response = self
            .client()?
            .list_tools(&request.toolkits, &request.tags)
            .await
            .map_err(|error| to_bus_error(&error))?;

        if request.apply_user_scopes {
            let mut allowed = Vec::with_capacity(response.tools.len());
            for tool in response.tools {
                if self.permits(&tool.function.name).await? {
                    allowed.push(tool);
                }
            }
            response.tools = allowed;
        }
        Ok(response)
    }

    async fn sync(&self, request: ConnectorSyncRequest) -> TinyBusResult<ConnectorSyncResponse> {
        let provider = self.registry.get(&request.toolkit).ok_or_else(|| {
            tinybus::Error::failed(format!(
                "no provider for toolkit `{}`: this build does not know how to read it",
                request.toolkit
            ))
        })?;

        let connection_id = match request.connection_id {
            Some(id) if !id.trim().is_empty() => id,
            _ => self.first_active_connection(&request.toolkit).await?,
        };
        let source_id = request
            .source_id
            .filter(|id| !id.trim().is_empty())
            .unwrap_or_else(|| format!("{}:{connection_id}", request.toolkit));

        let mut limits = SyncLimits::default();
        if let Some(max_items) = request.max_items.filter(|max| *max > 0) {
            limits.max_items = max_items;
        }

        let context = ProviderContext {
            toolkit: request.toolkit.clone(),
            connection_id,
            source_id,
            limits,
            actions: self.actions.clone(),
            state: self.state.clone(),
        };

        let outcome = run_sync(
            provider.as_ref(),
            &context,
            sync_reason(request.reason.as_deref()),
        )
        .await
        .map_err(|error| tinybus::Error::failed(error.to_string()))?;

        Ok(ConnectorSyncResponse {
            batch: outcome.batch,
            stage: outcome.stage,
            pages_read: outcome.pages_read,
            records_skipped: outcome.records_skipped,
            message: outcome.message,
        })
    }

    async fn get_user_scopes(
        &self,
        request: ComposioGetUserScopesRequest,
    ) -> TinyBusResult<ComposioUserScopesResponse> {
        let pref = UserScopePref::load(self.state.as_ref(), &request.toolkit)
            .await
            .map_err(|error| tinybus::Error::failed(error.to_string()))?;
        Ok(scopes_response(&request.toolkit, pref))
    }

    async fn set_user_scopes(
        &self,
        request: ComposioSetUserScopesRequest,
    ) -> TinyBusResult<ComposioUserScopesResponse> {
        let pref = UserScopePref {
            read: request.scopes.read,
            write: request.scopes.write,
            admin: request.scopes.admin,
        };
        pref.save(self.state.as_ref(), &request.toolkit)
            .await
            .map_err(|error| tinybus::Error::failed(error.to_string()))?;
        Ok(scopes_response(&request.toolkit, pref))
    }

    async fn execute(
        &self,
        request: ComposioExecuteRequest,
    ) -> TinyBusResult<ComposioExecuteResponse> {
        // Enforced here rather than trusted from the caller: a preference a
        // caller could opt out of is not a restriction, it is a suggestion.
        if !self.permits(&request.tool).await? {
            return Err(tinybus::Error::failed(format!(
                "`{}` is not permitted: the scope preference for this toolkit does not allow it",
                request.tool
            )));
        }
        self.client()?
            .execute(
                &request.tool,
                request.arguments,
                request.connection_id.as_deref(),
            )
            .await
            .map_err(|error| to_bus_error(&error))
    }

    // The registry is in memory: there is nothing to await. `async` is the
    // shape the interface macro dispatches, not a claim about the work.
    #[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
    async fn list_capabilities(&self) -> TinyBusResult<ComposioCapabilitiesResponse> {
        Ok(self.registry.capabilities())
    }

    // Same: reads the registry.
    #[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
    async fn list_agent_ready_toolkits(&self) -> TinyBusResult<ComposioAgentReadyToolkitsResponse> {
        Ok(ComposioAgentReadyToolkitsResponse {
            toolkits: self.registry.agent_ready_toolkits(),
        })
    }

    async fn get_user_profile(
        &self,
        request: ComposioUserProfileRequest,
    ) -> TinyBusResult<ComposioUserProfile> {
        let connection_id = match request.connection_id {
            Some(id) if !id.trim().is_empty() => id,
            // No connection named: use the toolkit's first active one. A user
            // with several gets an arbitrary answer, which is why a caller that
            // knows which it means should say so.
            _ => self.first_active_connection(&request.toolkit).await?,
        };
        self.profile_of(&request.toolkit, &connection_id).await
    }

    async fn refresh_all_identities(&self) -> TinyBusResult<ComposioRefreshIdentitiesResponse> {
        let connections = self
            .client()?
            .list_connections()
            .await
            .map_err(|error| to_bus_error(&error))?
            .connections;

        let mut profiles = Vec::new();
        let mut failures = Vec::new();
        for connection in connections.iter().filter(|c| c.is_active()) {
            let toolkit = connection.normalized_toolkit();
            match self.profile_of(&toolkit, &connection.id).await {
                Ok(profile) => profiles.push(profile),
                // One unreadable connection must not hide every readable one:
                // a refresh exists precisely to find the broken ones.
                Err(error) => failures.push(ComposioIdentityFailure {
                    connection_id: connection.id.clone(),
                    toolkit,
                    message: error.to_string(),
                }),
            }
        }

        Ok(ComposioRefreshIdentitiesResponse { profiles, failures })
    }

    async fn list_github_repos(
        &self,
        request: ComposioListGithubReposRequest,
    ) -> TinyBusResult<ComposioGithubReposResponse> {
        self.client()?
            .list_github_repos(request.connection_id.as_deref())
            .await
            .map_err(|error| to_bus_error(&error))
    }

    async fn list_available_triggers(
        &self,
        request: ComposioListAvailableTriggersRequest,
    ) -> TinyBusResult<ComposioAvailableTriggersResponse> {
        self.client()?
            .list_available_triggers(&request.toolkit, request.connection_id.as_deref())
            .await
            .map_err(|error| to_bus_error(&error))
    }

    async fn list_triggers(
        &self,
        request: ComposioListTriggersRequest,
    ) -> TinyBusResult<ComposioActiveTriggersResponse> {
        self.client()?
            .list_triggers(request.toolkit.as_deref())
            .await
            .map_err(|error| to_bus_error(&error))
    }

    async fn create_trigger(
        &self,
        request: ComposioCreateTriggerRequest,
    ) -> TinyBusResult<ComposioCreateTriggerResponse> {
        self.client()?
            .create_trigger(
                &request.slug,
                request.connection_id.as_deref(),
                request.trigger_config,
            )
            .await
            .map_err(|error| to_bus_error(&error))
    }

    async fn enable_trigger(
        &self,
        request: ComposioEnableTriggerRequest,
    ) -> TinyBusResult<ComposioEnableTriggerResponse> {
        self.client()?
            .enable_trigger(
                &request.connection_id,
                &request.slug,
                request.trigger_config,
            )
            .await
            .map_err(|error| to_bus_error(&error))
    }

    async fn disable_trigger(
        &self,
        request: ComposioDisableTriggerRequest,
    ) -> TinyBusResult<ComposioDisableTriggerResponse> {
        self.client()?
            .disable_trigger(&request.trigger_id)
            .await
            .map_err(|error| to_bus_error(&error))
    }

    async fn list_trigger_history(
        &self,
        request: ComposioListTriggerHistoryRequest,
    ) -> TinyBusResult<ComposioTriggerHistoryResult> {
        let archive = self
            .archive
            .as_ref()
            .ok_or_else(|| {
                tinybus::Error::failed(
                    "trigger history is unavailable: the module was loaded without a `state_dir`",
                )
            })?
            .clone();

        // Reading the archive is synchronous file I/O, and the module owns a
        // one-worker runtime: doing it inline would stall every other member
        // for the duration of the read.
        tokio::task::spawn_blocking(move || archive.list_recent(request.limit))
            .await
            .map_err(|error| tinybus::Error::failed(format!("history read failed: {error}")))?
            .map_err(|error| to_bus_error(&error))
    }
}

/// The state store for a host that named a directory, or an ephemeral one.
fn state_store(state_dir: Option<&std::path::Path>) -> Arc<dyn SyncStateStore> {
    match state_dir {
        Some(dir) => Arc::new(FileStateStore::new(dir)),
        None => Arc::new(EphemeralStateStore::default()),
    }
}

/// Sync state that lives only as long as the module.
///
/// A sync running on this re-reads a connection's history after every restart,
/// which is why a host that means to sync should name a `state_dir`. It exists
/// so the members that need no state — profiles, capabilities — work without
/// one, rather than making every deployment carry a path it does not use.
#[derive(Debug, Default)]
struct EphemeralStateStore {
    values: std::sync::Mutex<std::collections::HashMap<(String, String), serde_json::Value>>,
}

#[async_trait::async_trait]
impl SyncStateStore for EphemeralStateStore {
    async fn get(
        &self,
        namespace: &str,
        key: &str,
    ) -> tinyconnectors_sync::Result<Option<serde_json::Value>> {
        Ok(self
            .values
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&(namespace.to_string(), key.to_string()))
            .cloned())
    }

    async fn set(
        &self,
        namespace: &str,
        key: &str,
        value: &serde_json::Value,
    ) -> tinyconnectors_sync::Result<()> {
        self.values
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert((namespace.to_string(), key.to_string()), value.clone());
        Ok(())
    }
}

/// Why a caller says the run started.
///
/// An unrecognized reason is treated as manual rather than refused: the reason
/// is for a log line and a status label, and failing a sync over one would
/// break a working integration for a cosmetic field.
fn sync_reason(reason: Option<&str>) -> SyncReason {
    match reason.map(str::trim).unwrap_or_default() {
        "initial_connect" => SyncReason::InitialConnect,
        "scheduled" => SyncReason::Scheduled,
        "trigger" => SyncReason::Trigger,
        _ => SyncReason::Manual,
    }
}

/// Render a stored preference as the member's reply.
fn scopes_response(toolkit: &str, pref: UserScopePref) -> ComposioUserScopesResponse {
    ComposioUserScopesResponse {
        toolkit: toolkit.trim().to_ascii_lowercase(),
        scopes: ComposioUserScopes {
            read: pref.read,
            write: pref.write,
            admin: pref.admin,
        },
    }
}

/// Flatten a crate error onto the bus.
fn to_bus_error(error: &crate::Error) -> tinybus::Error {
    tinybus::Error::failed(error.to_string())
}

async fn setup(connection: Connection, config: ModuleConfig) -> TinyBusResult<()> {
    // Opened before the route so a bad state directory fails at load rather
    // than on the first trigger, weeks later.
    let config_state_dir = config.state_dir().map(std::path::Path::to_path_buf);
    let archive = config_state_dir
        .as_deref()
        .map(TriggerArchive::open)
        .transpose()
        .map_err(|error| to_bus_error(&error))?;

    let route = config.into_route().map_err(|error| to_bus_error(&error))?;
    tracing::info!(
        route = route.as_ref().map_or("none", |route| route.name()),
        archiving_triggers = archive.is_some(),
        "[connectors] serving connector surface"
    );
    let client = Arc::new(RwLock::new(route.map(ComposioClient::new)));
    let service = ConnectorService {
        // A module with no route still answers the capability members, so the
        // action runner is built over a client that reports the missing route
        // if a provider ever reaches it. It shares the handle rather than
        // copying it, so a later `Configure` reaches running syncs too.
        actions: Arc::new(ClientActions::new(Arc::clone(&client))),
        // A host that named no state directory gets an in-memory store: profile
        // and capability members need none, and a sync run without persistence
        // is better than a module that refuses to load.
        state: state_store(config_state_dir.as_deref()),
        registry: crate::providers::default_registry(),
        client,
        archive,
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
        "Configure",
        "ListToolkits",
        "ListConnections",
        "Authorize",
        "DeleteConnection",
        "ListTools",
        "Sync",
        "GetUserScopes",
        "SetUserScopes",
        "Execute",
        "ListCapabilities",
        "ListAgentReadyToolkits",
        "GetUserProfile",
        "RefreshAllIdentities",
        "ListGithubRepos",
        "ListAvailableTriggers",
        "ListTriggers",
        "CreateTrigger",
        "EnableTrigger",
        "DisableTrigger",
        "ListTriggerHistory",
    ],
    signals = [],
    requires = [],
    optional = [],
    lazy = false,
}

#[cfg(test)]
mod test;
