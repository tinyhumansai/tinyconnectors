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
//! The host supplies `ModuleConfig` as the module's JSON configuration blob
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

mod contracts;
mod restricted;
mod routed;

use std::sync::{Arc, RwLock};

use serde::Deserialize;
use tinybus::{Connection, Result as TinyBusResult};
use tinyconnectors_bus::{
    ComposioActiveTriggersResponse, ComposioAgentReadyToolkitsResponse, ComposioAuthorizeRequest,
    ComposioAuthorizeResponse, ComposioAvailableTriggersResponse, ComposioCapabilitiesResponse,
    ComposioConfigureRequest, ComposioConfigureResponse, ComposioConnectionsResponse,
    ComposioCreateTriggerRequest, ComposioCreateTriggerResponse, ComposioDeleteConnectionRequest,
    ComposioDeleteResponse, ComposioDirectConnectionsRequest, ComposioDirectToolsRequest,
    ComposioDisableTriggerRequest, ComposioDisableTriggerResponse, ComposioEnableTriggerRequest,
    ComposioEnableTriggerResponse, ComposioExecuteRequest, ComposioExecuteResponse,
    ComposioGetUserScopesRequest, ComposioGithubReposResponse, ComposioIdentityFailure,
    ComposioListAvailableTriggersRequest, ComposioListGithubReposRequest, ComposioListToolsRequest,
    ComposioListTriggerHistoryRequest, ComposioListTriggersRequest,
    ComposioRefreshIdentitiesResponse, ComposioSetUserScopesRequest, ComposioToolkitsResponse,
    ComposioToolsResponse, ComposioTransportConfig, ComposioTriggerHistoryResult,
    ComposioUserProfile, ComposioUserProfileRequest, ComposioUserScopes,
    ComposioUserScopesResponse, names,
};

use tinyconnectors_bus::{
    ClassifyErrorRequest, ComposioTriggerHistoryEntry, FilterResponseRequest,
    PrepareArgumentsRequest, PreparedArguments, ProviderError,
};

use crate::client::{
    COMPOSIO_API_BASE, ComposioClient, DirectRoute, HttpTransport, ProxyRoute, Route,
};
use tinyconnectors_sync::{
    PrefsStore, ProviderContext, ProviderRegistry, ScopePrefStore, UserScopePref, classify_unknown,
    find_curated,
};

use crate::providers::ClientActions;
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
/// { "route": "direct", "api_key": "…", "entity_id": "default", "webhooks": false }
/// ```
///
/// Reachable only through `module_export!`, which names the type in the ABI
/// entrypoint it generates; nothing re-exports it from the crate root.
#[derive(Clone, Deserialize)]
#[serde(tag = "route", rename_all = "snake_case")]
pub(crate) enum RouteConfig {
    /// Reach Composio through the `TinyHumans` backend.
    Proxy {
        /// Accepted for wire compatibility and ignored. The trigger archive and
        /// preferences live in the host's storage, reached over the storage
        /// bus; the module never writes to a directory.
        #[serde(default)]
        #[allow(
            dead_code,
            reason = "parsed for wire compatibility; storage comes from the bus"
        )]
        state_dir: Option<std::path::PathBuf>,
        /// Base URL of the connector backend, e.g. `https://api.example.com`.
        base_url: String,
        /// Bearer token for the signed-in user.
        ///
        /// Never logged and never returned through a member — see
        /// `HttpTransport`'s hand-written `Debug`.
        auth_token: String,
        /// The user's IANA time zone, sent to the backend as `x-timezone`.
        /// Optional: without it the backend renders UTC, as it always has.
        #[serde(default)]
        timezone: Option<String>,
        /// Proxy and TLS settings for the backend connection. Optional: absent
        /// means no proxy and bundled roots.
        #[serde(default)]
        transport: Option<ComposioTransportConfig>,
    },
    /// Reach Composio directly with a user-supplied key.
    Direct {
        /// Accepted for symmetry with the proxy variant and ignored.
        #[serde(default)]
        #[allow(
            dead_code,
            reason = "parsed for wire compatibility; storage comes from the bus"
        )]
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
        /// Proxy and TLS settings for the connection to Composio. Optional:
        /// absent means no proxy and bundled roots.
        #[serde(default)]
        transport: Option<ComposioTransportConfig>,
        /// Whether the host receives Composio's webhook deliveries at an
        /// endpoint of its own. Optional: absent means `false`, and the trigger
        /// members refuse — see [`DirectRoute::with_webhooks`].
        #[serde(default)]
        webhooks: bool,
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
    runtime_mode: tinyconnectors_bus::ComposioRuntimeMode,
    /// The toolkits the host offers its agent. Applied to listings only.
    offered: tinyconnectors_bus::vocabulary::OfferedToolkits,
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
        let runtime_mode = value
            .get("runtime_mode")
            .cloned()
            .map(serde_json::from_value::<tinyconnectors_bus::ComposioRuntimeMode>)
            .transpose()
            .map_err(serde::de::Error::custom)?
            .unwrap_or_default();
        let offered = value
            .get("offered_toolkits")
            .cloned()
            .map(serde_json::from_value::<tinyconnectors_bus::vocabulary::OfferedToolkits>)
            .transpose()
            .map_err(serde::de::Error::custom)?
            .unwrap_or_default();
        let route = if value.get("route").is_none_or(serde_json::Value::is_null) {
            None
        } else {
            Some(RouteConfig::deserialize(value.clone()).map_err(serde::de::Error::custom)?)
        };
        if runtime_mode == tinyconnectors_bus::ComposioRuntimeMode::RoutedReadOnly
            && (route.is_some() || value.get("state_dir").is_some())
        {
            return Err(serde::de::Error::custom(
                "routed_read_only does not admit a stored route or state directory",
            ));
        }
        Ok(Self {
            route,
            runtime_mode,
            offered,
        })
    }
}

impl ModuleConfig {
    /// The `state_dir` the host sent, which is parsed for wire compatibility and
    /// otherwise unused: storage comes from the host's storage bus.
    #[cfg(test)]
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
                timezone,
                transport,
            } => Some(Self::Proxy {
                base_url,
                auth_token,
                timezone,
                transport,
                state_dir: None,
            }),
            ComposioConfigureRequest::Direct {
                api_key,
                entity_id,
                base_url,
                transport,
                webhooks,
            } => Some(Self::Direct {
                api_key,
                entity_id,
                base_url,
                transport,
                webhooks,
                state_dir: None,
            }),
        }
    }
}

impl RouteConfig {
    #[cfg(test)]
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
                timezone,
                transport,
                ..
            } => {
                let transport = Arc::new(
                    HttpTransport::bearer(&base_url, auth_token)?
                        .with_timezone(timezone.as_deref())
                        .with_network(transport.as_ref())?,
                );
                Ok(Arc::new(ProxyRoute::new(transport)))
            }
            Self::Direct {
                api_key,
                entity_id,
                base_url,
                transport,
                webhooks,
                ..
            } => {
                let base_url = base_url.unwrap_or_else(|| COMPOSIO_API_BASE.to_string());
                let transport = Arc::new(
                    HttpTransport::api_key(&base_url, api_key.clone())?
                        .with_network(transport.as_ref())?,
                );
                Ok(Arc::new(
                    DirectRoute::new(transport, &api_key, entity_id.unwrap_or_default())
                        .with_webhooks(webhooks),
                ))
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
    /// The host's storage over the storage bus: the trigger archive's streams
    /// and the preference documents. Connected on first use, so a host that
    /// never enables triggers pays nothing and a host whose storage comes up
    /// after the module loads is still reached.
    storage: storage::StorageLink,
    archives: Arc<archives::Archives>,
    /// The toolkits this build knows.
    ///
    /// Answers the capability members without touching the network, and gives
    /// the profile members the action slug and identity field for a toolkit.
    registry: ProviderRegistry,
    /// How providers run their actions.
    actions: Arc<ClientActions>,
    /// Where the per-toolkit scope preferences are persisted.
    prefs: Arc<dyn PrefsStore>,
    /// Tool contracts and probe results remembered between calls.
    contract_caches: Arc<crate::contracts::ContractCaches>,
    /// Which toolkits listings offer; fixed at load.
    offered: tinyconnectors_bus::vocabulary::OfferedToolkits,
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
            actions: self.actions.clone(),
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

    /// The preference governing `action`: the one saved under the longest
    /// toolkit name its slug starts with, or the default if none is.
    ///
    /// Matched against what was saved rather than guessed from the slug: a
    /// toolkit whose own name contains `_` (`zoho_books`) would otherwise be
    /// read under its first segment (`zoho`), and its read-only preference
    /// never consulted. A store or decode failure still refuses.
    async fn pref_for(&self, action: &str) -> TinyBusResult<UserScopePref> {
        let failed = |error: tinyconnectors_sync::Error| tinybus::Error::failed(error.to_string());
        let slug = action.trim().to_ascii_lowercase();
        for (end, _) in slug.rmatch_indices('_') {
            let toolkit = &slug[..end];
            if self
                .prefs
                .get(&UserScopePref::key(toolkit))
                .await
                .map_err(failed)?
                .is_some()
            {
                return UserScopePref::load(self.prefs.as_ref(), toolkit)
                    .await
                    .map_err(failed);
            }
        }
        Ok(UserScopePref::default())
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
        let pref = self.pref_for(action).await?;

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

    /// Whether a listing offers `action` under the stored scope preference.
    ///
    /// Stricter than [`Self::permits`] in one way: a toolkit with a curated
    /// catalog offers only the actions in it, so the agent never sees an action
    /// the catalog does not vouch for. Execution keeps the preference-only rule.
    async fn listed(&self, action: &str) -> TinyBusResult<bool> {
        if tinyconnectors_sync::toolkit_from_slug(action).is_none() {
            return Ok(true);
        }
        let pref = self.pref_for(action).await?;
        Ok(crate::vocabulary_impl::catalogs::is_action_visible_with_pref(action, pref))
    }

    /// Runs one action after enforcing the stored scope preference.
    ///
    /// Enforced here rather than trusted from the caller: a preference a
    /// caller could opt out of is not a restriction, it is a suggestion.
    /// Shared by `Execute` and the output probe so neither can bypass it.
    async fn execute_checked(
        &self,
        tool: &str,
        arguments: Option<serde_json::Value>,
        connection_id: Option<&str>,
    ) -> TinyBusResult<ComposioExecuteResponse> {
        if !self.permits(tool).await? {
            return Err(tinybus::Error::failed(format!(
                "`{tool}` is not permitted: the scope preference for this toolkit does not allow it"
            )));
        }
        self.client()?
            .execute(tool, arguments, connection_id)
            .await
            // Classified, like a refusal already is. A provider that answers
            // "insufficient scope" and a backend that times out mean different
            // things to whoever is looking at the screen — one is "reconnect
            // your account", the other is "try again" — and the `[composio:
            // error:<class>]` prefix is how a caller tells them apart.
            //
            // Only the `Err` path passes through here, which is transport and
            // decode failures. A provider *refusal* arrives as a successful
            // reply carrying `successful: false`, and its message is formatted
            // where that is decided, so nothing is classified twice.
            .map_err(|error| {
                tinybus::Error::failed(crate::execute::format_provider_error(
                    tool,
                    &error.to_string(),
                ))
            })
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
    #[allow(clippy::unused_async, reason = "required by the interface dispatcher")]
    #[allow(unknown_lints, clippy::unused_async_trait_impl)] // the bus trait method is async; this body has nothing to await
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
        // A catalog fetched for the old tenant must not answer for the new one.
        self.contract_caches.clear();
        Ok(ComposioConfigureResponse { route: name })
    }

    async fn list_toolkits(&self) -> TinyBusResult<ComposioToolkitsResponse> {
        let mut response = self
            .client()?
            .list_toolkits()
            .await
            .map_err(|error| to_bus_error(&error))?;
        response
            .toolkits
            .retain(|toolkit| self.offered.permits(toolkit));
        response
            .catalog
            .retain(|entry| self.offered.permits(&entry.slug));
        Ok(response)
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

        response.tools.retain(|tool| {
            tinyconnectors_sync::toolkit_from_slug(&tool.function.name)
                .is_none_or(|toolkit| self.offered.permits(&toolkit))
        });

        if request.apply_user_scopes {
            let mut allowed = Vec::with_capacity(response.tools.len());
            for tool in response.tools {
                if self.listed(&tool.function.name).await? {
                    allowed.push(tool);
                }
            }
            response.tools = allowed;
        }
        Ok(response)
    }

    async fn get_user_scopes(
        &self,
        request: ComposioGetUserScopesRequest,
    ) -> TinyBusResult<ComposioUserScopesResponse> {
        let pref = UserScopePref::load(self.prefs.as_ref(), &request.toolkit)
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
        pref.save(self.prefs.as_ref(), &request.toolkit)
            .await
            .map_err(|error| tinybus::Error::failed(error.to_string()))?;
        Ok(scopes_response(&request.toolkit, pref))
    }

    // Pure preparation is synchronous; async signatures are required by TinyBus.
    #[allow(
        clippy::unused_async,
        reason = "TinyBus interface signatures are async; this operation performs synchronous in-memory work"
    )]
    async fn prepare_arguments(
        &self,
        request: PrepareArgumentsRequest,
    ) -> TinyBusResult<PreparedArguments> {
        let result = processing::prepare(request);
        Ok(result)
    }

    #[allow(
        clippy::unused_async,
        reason = "TinyBus interface signatures are async; this operation performs synchronous in-memory work"
    )]
    async fn filter_response(
        &self,
        request: FilterResponseRequest,
    ) -> TinyBusResult<ComposioExecuteResponse> {
        let since = processing::parse_since(&request.since)?;
        Ok(crate::execute::filter_response(
            &request.tool,
            request.response,
            since,
        ))
    }

    #[allow(
        clippy::unused_async,
        reason = "TinyBus interface signatures are async; this operation performs synchronous in-memory work"
    )]
    async fn classify_error(&self, request: ClassifyErrorRequest) -> TinyBusResult<ProviderError> {
        Ok(processing::classified(&request.tool, &request.message))
    }

    async fn record_trigger(
        &self,
        request: tinyconnectors_bus::RecordTriggerRequest,
    ) -> TinyBusResult<ComposioTriggerHistoryEntry> {
        let archive = self.archives.get(&request.handle)?;
        let event = request.event;
        archive
            .record(
                &event.toolkit,
                &event.trigger,
                &event.metadata.id,
                &event.metadata.uuid,
                &event.payload,
            )
            .await
            .map_err(|_| tinybus::Error::failed("trigger archive write failed"))
    }

    async fn open_archive(
        &self,
        _request: tinyconnectors_bus::OpenArchiveRequest,
    ) -> TinyBusResult<tinyconnectors_bus::ArchiveHandle> {
        // `state_dir` is kept on the wire for compatibility and ignored: the
        // host binds the scope from the authenticated caller.
        let streams = self.storage.streams().await.map_err(|error| {
            tracing::debug!(%error, "[connectors][triggers] archive storage unavailable");
            tinybus::Error::failed("trigger archive storage unavailable")
        })?;
        self.archives.open(streams)
    }

    async fn read_archive(
        &self,
        request: tinyconnectors_bus::ReadArchiveRequest,
    ) -> TinyBusResult<ComposioTriggerHistoryResult> {
        let archive = self.archives.get(&request.handle)?;
        archive
            .list_recent(request.limit)
            .await
            .map_err(|_| tinybus::Error::failed("trigger archive read failed"))
    }

    #[allow(
        clippy::unused_async,
        reason = "TinyBus interface signatures are async; this operation performs synchronous in-memory work"
    )]
    async fn close_archive(&self, handle: tinyconnectors_bus::ArchiveHandle) -> TinyBusResult<()> {
        self.archives.close(&handle)
    }

    async fn execute(
        &self,
        request: ComposioExecuteRequest,
    ) -> TinyBusResult<ComposioExecuteResponse> {
        self.execute_checked(
            &request.tool,
            request.arguments,
            request.connection_id.as_deref(),
        )
        .await
    }

    // The registry is in memory: there is nothing to await. `async` is the
    // shape the interface macro dispatches, not a claim about the work.
    #[allow(clippy::unused_async)]
    #[allow(unknown_lints, clippy::unused_async_trait_impl)] // the bus trait method is async; this body has nothing to await
    async fn list_capabilities(&self) -> TinyBusResult<ComposioCapabilitiesResponse> {
        Ok(self.registry.capabilities())
    }

    // Same: reads the registry.
    #[allow(clippy::unused_async)]
    #[allow(unknown_lints, clippy::unused_async_trait_impl)] // the bus trait method is async; this body has nothing to await
    async fn list_agent_ready_toolkits(&self) -> TinyBusResult<ComposioAgentReadyToolkitsResponse> {
        let mut toolkits = self.registry.agent_ready_toolkits();
        toolkits.retain(|toolkit| self.offered.permits(toolkit));
        Ok(ComposioAgentReadyToolkitsResponse { toolkits })
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
            // Enabling is the one trigger call a user drives from a settings
            // screen, so a failure is classified like an execute failure
            // (`[composio:error:<class>]`): "reconnect GitHub" reads better
            // than the provider's own wording.
            .map_err(|error| {
                tinybus::Error::failed(crate::execute::format_provider_error(
                    &request.slug,
                    &error.to_string(),
                ))
            })
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
        let streams = self.storage.streams().await.map_err(|error| {
            tracing::debug!(%error, "[connectors][triggers] history storage unavailable");
            tinybus::Error::failed(
                "trigger history is unavailable: the host does not serve storage to this module",
            )
        })?;

        TriggerArchive::new(streams)
            .list_recent(request.limit)
            .await
            .map_err(|error| to_bus_error(&error))
    }

    /// List connections with the credential the request carries.
    ///
    /// Reads and replaces nothing: the configured route is untouched and the
    /// key is dropped when the call returns. The failure is the message a user
    /// reads, verbatim (see [`crate::client::list_connections_direct`]), so a
    /// host can tell a rejected key from an outage.
    async fn list_connections_direct(
        &self,
        request: ComposioDirectConnectionsRequest,
    ) -> TinyBusResult<ComposioConnectionsResponse> {
        crate::client::list_connections_direct(&request.credential)
            .await
            .map_err(tinybus::Error::failed)
    }

    /// List tools with the credential the request carries. Stateless like
    /// `ListConnectionsDirect`, and unfiltered by the user's scope preference.
    async fn list_tools_direct(
        &self,
        request: ComposioDirectToolsRequest,
    ) -> TinyBusResult<ComposioToolsResponse> {
        crate::client::list_tools_direct(&request.credential, &request.toolkits, &request.tags)
            .await
            .map_err(tinybus::Error::failed)
    }
    /// Read through only the request's immutable confidential route.
    #[tinybus(confidential)]
    async fn routed_read(
        &self,
        request: tinyconnectors_bus::ComposioRoutedReadRequest,
    ) -> TinyBusResult<tinyconnectors_bus::ComposioRoutedReadReply> {
        routed::read(request).await
    }
    /// Returns portable catalogs without credentials or a backend call.
    #[allow(clippy::unused_async, reason = "required by the interface dispatcher")]
    #[allow(unknown_lints, clippy::unused_async_trait_impl)]
    async fn get_vocabulary(
        &self,
        request: tinyconnectors_bus::vocabulary::VocabularyRequest,
    ) -> TinyBusResult<tinyconnectors_bus::vocabulary::VocabularyResponse> {
        let mut vocabulary = crate::vocabulary_impl::get_vocabulary(request);
        vocabulary
            .agent_ready_toolkits
            .retain(|toolkit| self.offered.permits(toolkit));
        Ok(vocabulary)
    }

    /// Lists the curated action catalog without credentials or a backend call.
    #[allow(clippy::unused_async, reason = "required by the interface dispatcher")]
    #[allow(unknown_lints, clippy::unused_async_trait_impl)]
    async fn list_curated_catalog(
        &self,
        request: tinyconnectors_bus::vocabulary::CuratedCatalogRequest,
    ) -> TinyBusResult<Vec<tinyconnectors_bus::vocabulary::CuratedTool>> {
        Ok(crate::vocabulary_impl::list_curated_catalog(request))
    }

    /// Lists the curated actions a scope preference exposes for one toolkit.
    ///
    /// Without a preference in the request, the user's stored one applies, so
    /// the answer matches what `ListTools` and `Execute` enforce.
    async fn list_visible_tools(
        &self,
        request: tinyconnectors_bus::vocabulary::VisibleToolsRequest,
    ) -> TinyBusResult<tinyconnectors_bus::vocabulary::VisibleToolsResponse> {
        let toolkit = request.toolkit.trim().to_ascii_lowercase();
        let preference = match request.preference {
            Some(preference) => preference,
            None => UserScopePref::load(self.prefs.as_ref(), &toolkit)
                .await
                .map_err(|error| tinybus::Error::failed(error.to_string()))?,
        };
        Ok(crate::vocabulary_impl::list_visible_tools(
            &toolkit, preference,
        ))
    }

    /// Evaluates display metadata; Execute still checks persisted preferences.
    #[allow(clippy::unused_async, reason = "required by the interface dispatcher")]
    #[allow(unknown_lints, clippy::unused_async_trait_impl)]
    async fn evaluate_actions(
        &self,
        request: Vec<tinyconnectors_bus::vocabulary::ActionVocabularyRequest>,
    ) -> TinyBusResult<Vec<tinyconnectors_bus::vocabulary::ActionVocabularyResponse>> {
        Ok(crate::vocabulary_impl::evaluate_actions(request))
    }

    /// Canonicalizes identifiers and renders the unchanged identity prompt.
    #[allow(clippy::unused_async, reason = "required by the interface dispatcher")]
    #[allow(unknown_lints, clippy::unused_async_trait_impl)]
    async fn normalize_identities(
        &self,
        request: tinyconnectors_bus::vocabulary::NormalizeIdentitiesRequest,
    ) -> TinyBusResult<tinyconnectors_bus::vocabulary::NormalizeIdentitiesResponse> {
        Ok(crate::vocabulary_impl::normalize_identities(request))
    }

    /// Applies the portable unset-cap rule without claiming a fetch capability.
    #[allow(clippy::unused_async, reason = "required by the interface dispatcher")]
    #[allow(unknown_lints, clippy::unused_async_trait_impl)]
    async fn prepare_task_filter(
        &self,
        request: tinyconnectors_bus::vocabulary::TaskFetchFilter,
    ) -> TinyBusResult<tinyconnectors_bus::vocabulary::PreparedTaskFilter> {
        Ok(crate::vocabulary_impl::prepare_task_filter(request))
    }

    /// Returns one action's live tool contract, probe overlay included.
    async fn get_tool_contract(
        &self,
        request: tinyconnectors_bus::composio::ComposioGetToolContractRequest,
    ) -> TinyBusResult<tinyconnectors_bus::composio::ComposioGetToolContractResponse> {
        Ok(
            tinyconnectors_bus::composio::ComposioGetToolContractResponse {
                contract: self.contract_with_probe(&request.slug).await?,
            },
        )
    }

    /// Returns every live tool contract of one toolkit, probe overlay included.
    async fn list_tool_contracts(
        &self,
        request: tinyconnectors_bus::composio::ComposioListToolContractsRequest,
    ) -> TinyBusResult<tinyconnectors_bus::composio::ComposioListToolContractsResponse> {
        let (toolkit, contracts) = self.toolkit_contracts(&request.toolkit).await?;
        Ok(
            tinyconnectors_bus::composio::ComposioListToolContractsResponse {
                toolkit,
                contracts: contracts
                    .into_iter()
                    .map(|contract| self.contract_caches.apply_probe(contract))
                    .collect(),
            },
        )
    }

    /// Runs one confirmed read action and reports the real output shape.
    async fn probe_tool_output(
        &self,
        request: tinyconnectors_bus::composio::ComposioProbeToolOutputRequest,
    ) -> TinyBusResult<tinyconnectors_bus::composio::ComposioProbedOutputSample> {
        self.probe_output(&request.slug, request.args).await
    }

    /// Assembles the connected-integration overview for one route.
    async fn list_connected_integrations(
        &self,
        request: tinyconnectors_bus::composio::ComposioListConnectedIntegrationsRequest,
    ) -> TinyBusResult<tinyconnectors_bus::composio::ComposioConnectedIntegrationsResponse> {
        Ok(
            tinyconnectors_bus::composio::ComposioConnectedIntegrationsResponse {
                integrations: self.connected_integrations(request.mode).await?,
            },
        )
    }

    #[allow(
        clippy::unused_async,
        reason = "TinyBus methods are async; mode is immutable"
    )]
    async fn runtime_mode(&self) -> TinyBusResult<tinyconnectors_bus::ComposioRuntimeModeResponse> {
        Ok(tinyconnectors_bus::ComposioRuntimeModeResponse {
            mode: tinyconnectors_bus::ComposioRuntimeMode::Classic,
            contract_version: tinyconnectors_bus::CONTRACT_VERSION,
        })
    }
}

/// Test-only preference store for service logic that does not exercise storage.
#[cfg(test)]
#[derive(Debug, Default)]
struct EphemeralPrefsStore {
    values: std::sync::Mutex<std::collections::HashMap<String, serde_json::Value>>,
}

#[async_trait::async_trait]
#[cfg(test)]
impl PrefsStore for EphemeralPrefsStore {
    async fn get(&self, key: &str) -> tinyconnectors_sync::Result<Option<serde_json::Value>> {
        Ok(self
            .values
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(key)
            .cloned())
    }

    async fn set(&self, key: &str, value: &serde_json::Value) -> tinyconnectors_sync::Result<()> {
        self.values
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key.to_string(), value.clone());
        Ok(())
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
    if config.runtime_mode == tinyconnectors_bus::ComposioRuntimeMode::RoutedReadOnly {
        return restricted::setup(connection).await;
    }

    // `state_dir` is accepted for wire compatibility and unused: storage is
    // reached through the host's storage bus, scoped by the host.
    let storage = storage::StorageLink::new(connection.clone());

    let offered = config.offered.clone();
    let route = config.into_route().map_err(|error| to_bus_error(&error))?;
    tracing::info!(
        route = route.as_ref().map_or("none", |route| route.name()),
        "[connectors] serving connector surface"
    );
    let client = Arc::new(RwLock::new(route.map(ComposioClient::new)));
    let service = ConnectorService {
        // A module with no route still answers the capability members, so the
        // action runner is built over a client that reports the missing route
        // if a provider ever reaches it. It shares the handle rather than
        // copying it, so a later `Configure` reaches it too.
        actions: Arc::new(ClientActions::new(Arc::clone(&client))),
        // Preferences go to the host's document port; a missing storage
        // service fails the preference operation instead of losing state.
        prefs: Arc::new(storage::LinkedPrefsStore::new(storage.clone())),
        registry: crate::providers::default_registry(),
        contract_caches: Arc::default(),
        offered,
        client,
        storage,
        archives: Arc::default(),
    };

    connection
        .serve_at(names::OBJECT_PATH.try_into()?, service)
        .await?;
    connection.request_name(names::INTERFACE).await?;
    Ok(())
}

type SetupFuture = std::pin::Pin<Box<dyn std::future::Future<Output = TinyBusResult<()>> + Send>>;

/// Capture the first admitted mode for this ABI instance. `TinyBus` retains the
/// same callback for reinitialization, so another instance has its own seal.
fn setup_factory() -> impl Fn(Connection, ModuleConfig) -> SetupFuture + Send + Sync {
    let mode = Arc::new(tokio::sync::Mutex::new(None));
    move |connection, config: ModuleConfig| {
        let mode = mode.clone();
        Box::pin(async move {
            let mut admitted = mode.lock().await;
            if admitted.is_some_and(|initial| initial != config.runtime_mode) {
                return Err(tinybus::Error::failed(
                    "runtime mode cannot change after module initialization",
                ));
            }
            *admitted = Some(config.runtime_mode);
            drop(admitted);
            setup(connection, config).await
        })
    }
}

macro_rules! export_module {
    ($($declaration:tt)*) => {
        mod exports {
            // TinyBus 0.3 documents the ABI items generated below.
            use super::*;
            tinybus_module::module_export_optional_static! { $($declaration)* }
        }
        #[cfg(feature = "static-link")]
        pub use exports::{
            TINYBUS_MODULE_ABI_V1, linked_module, tinybus_module_init_v1,
            tinybus_module_manifest_v1,
        };
    };
}

export_module! {
    setup_factory = setup_factory,
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
        "GetUserScopes",
        "SetUserScopes",
        "PrepareArguments",
        "FilterResponse",
        "ClassifyError",
        "RecordTrigger",
        "OpenArchive",
        "ReadArchive",
        "CloseArchive",

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
        "ListConnectionsDirect",
        "ListToolsDirect",
        "RoutedRead",
        "GetVocabulary",
        "ListCuratedCatalog",
        "ListVisibleTools",
        "EvaluateActions",
        "NormalizeIdentities",
        "PrepareTaskFilter",
        "GetToolContract",
        "ListToolContracts",
        "ProbeToolOutput",
        "ListConnectedIntegrations",
        "RuntimeMode",

    ],
    signals = [],
    requires = [],
    optional = [],
    lazy = false,
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod test;

#[cfg(test)]
#[path = "direct_tests.rs"]
mod direct_test;

mod processing;

mod archives;
mod storage;

impl std::fmt::Debug for RouteConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Proxy { .. } => f.write_str("RouteConfig::Proxy(<redacted>)"),
            Self::Direct { .. } => f.write_str("RouteConfig::Direct(<redacted>)"),
        }
    }
}

#[cfg(test)]
#[path = "contracts_tests.rs"]
mod contracts_test;
