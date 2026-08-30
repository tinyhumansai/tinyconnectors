//! The bus identity of the `TinyConnectors` module: interface name, object path,
//! and one constant per member.
//!
//! Nothing here is a string literal at a call site. A host names a member
//! through [`methods`] and the object through [`OBJECT_PATH`], so a rename is a
//! compile error in every consumer rather than a runtime "unknown method".
//!
//! # Why the interface is named for Composio
//!
//! The crate is backend-neutral; this interface is not. Its members mirror
//! Composio's connector surface — toolkits, its OAuth handoff, its trigger
//! subscriptions — and a second backend will not answer them identically.
//! Rather than widen these members into a lowest common denominator, a second
//! backend gets its own interface and object path beside this one, and a host
//! picks the object it means. That is why the path has a `connectors/` segment
//! above the backend name: the room is already there.

/// The well-known interface name the module claims on the bus.
pub const INTERFACE: &str = "ai.tinyhumans.connectors.Composio";

/// The object path the module serves its interface at.
pub const OBJECT_PATH: &str = "/ai/tinyhumans/connectors/Composio";

/// One constant per member of [`INTERFACE`].
///
/// The table is the connector surface as it stands, not as it is planned. A
/// member appears here when the module serves it; the remaining Composio
/// operations — tools, execute, triggers, scopes, mode — arrive as additive
/// minor bumps of [`crate::CONTRACT_VERSION`] rather than as constants nothing
/// answers, which a host would only discover as a runtime "unknown method".
pub mod methods {
    /// Lists the toolkits the backend allowlist currently enables.
    ///
    /// Takes no arguments and returns a [`crate::ComposioToolkitsResponse`].
    pub const LIST_TOOLKITS: &str = "ListToolkits";

    /// Lists the caller's connections, active or not.
    ///
    /// Takes no arguments and returns a [`crate::ComposioConnectionsResponse`].
    /// Non-active rows are included: they are what an abandoned OAuth handoff
    /// leaves behind, and a caller cleaning them up needs to see them.
    pub const LIST_CONNECTIONS: &str = "ListConnections";

    /// Begins an OAuth handoff for a toolkit.
    ///
    /// Takes a [`crate::ComposioAuthorizeRequest`] and returns a
    /// [`crate::ComposioAuthorizeResponse`] carrying the hosted URL the user
    /// opens in a browser. The connection it names stays inactive until they
    /// finish, so a caller polls [`LIST_CONNECTIONS`] rather than expecting
    /// this member to block.
    pub const AUTHORIZE: &str = "Authorize";

    /// Disconnects a connection.
    ///
    /// Takes a [`crate::ComposioDeleteConnectionRequest`] and returns a
    /// [`crate::ComposioDeleteResponse`].
    pub const DELETE_CONNECTION: &str = "DeleteConnection";

    /// Lists the callable tools for a set of toolkits.
    ///
    /// Takes a [`crate::ComposioListToolsRequest`] and returns a
    /// [`crate::ComposioToolsResponse`] in the function-calling envelope a
    /// model expects, so a caller can forward it into a model request as-is.
    pub const LIST_TOOLS: &str = "ListTools";

    /// Runs one action against a connected account.
    ///
    /// Takes a [`crate::ComposioExecuteRequest`] and returns a
    /// [`crate::ComposioExecuteResponse`].
    ///
    /// A provider that refuses the call is **not** a member failure: the
    /// response carries `successful: false` and a formatted `error`. A member
    /// failure means the call never reached the provider at all. Callers that
    /// check only for a member error will report failed sends as successes.
    pub const EXECUTE: &str = "Execute";

    /// Lists the repositories a connected `GitHub` account can see.
    ///
    /// Takes a [`crate::ComposioListGithubReposRequest`] and returns a
    /// [`crate::ComposioGithubReposResponse`]. Exists to pick a repository for
    /// a `GitHub`-scoped trigger, whose subscriptions are per repository rather
    /// than per toolkit.
    pub const LIST_GITHUB_REPOS: &str = "ListGithubRepos";

    /// Lists the triggers a toolkit offers.
    ///
    /// Takes a [`crate::ComposioListAvailableTriggersRequest`] and returns a
    /// [`crate::ComposioAvailableTriggersResponse`].
    pub const LIST_AVAILABLE_TRIGGERS: &str = "ListAvailableTriggers";

    /// Lists the caller's enabled trigger subscriptions.
    ///
    /// Takes a [`crate::ComposioListTriggersRequest`] and returns a
    /// [`crate::ComposioActiveTriggersResponse`].
    pub const LIST_TRIGGERS: &str = "ListTriggers";

    /// Creates a trigger subscription.
    ///
    /// Takes a [`crate::ComposioCreateTriggerRequest`] and returns a
    /// [`crate::ComposioCreateTriggerResponse`].
    pub const CREATE_TRIGGER: &str = "CreateTrigger";

    /// Enables a trigger subscription on a connection.
    ///
    /// Takes a [`crate::ComposioEnableTriggerRequest`] and returns a
    /// [`crate::ComposioEnableTriggerResponse`].
    pub const ENABLE_TRIGGER: &str = "EnableTrigger";

    /// Disables a trigger subscription.
    ///
    /// Takes a [`crate::ComposioDisableTriggerRequest`] and returns a
    /// [`crate::ComposioDisableTriggerResponse`].
    pub const DISABLE_TRIGGER: &str = "DisableTrigger";

    /// Reads recent webhook deliveries from the module's archive.
    ///
    /// Takes a [`crate::ComposioListTriggerHistoryRequest`] and returns a
    /// [`crate::ComposioTriggerHistoryResult`].
    ///
    /// Answered from the module's own record, not from the backend: a delivery
    /// is fanned out over a socket and then gone, so the only way to answer
    /// "did it fire?" is to have written it down as it arrived.
    pub const LIST_TRIGGER_HISTORY: &str = "ListTriggerHistory";

    /// Reads a connected account's identity.
    ///
    /// Takes a [`crate::ComposioUserProfileRequest`] and returns a
    /// [`crate::ComposioUserProfile`]. Answered by the toolkit's provider,
    /// which knows both the action to call and the field the identity is in —
    /// so a toolkit with no provider cannot answer it.
    pub const GET_USER_PROFILE: &str = "GetUserProfile";

    /// Re-reads the identity of every connection.
    ///
    /// Takes no arguments and returns a
    /// [`crate::ComposioRefreshIdentitiesResponse`]. A connection whose profile
    /// could not be read is reported as a failure alongside the ones that
    /// worked, rather than failing the whole call: one broken connection must
    /// not hide every working one.
    pub const REFRESH_ALL_IDENTITIES: &str = "RefreshAllIdentities";

    /// Lists what this build can do for each toolkit it knows.
    ///
    /// Takes no arguments and returns a
    /// [`crate::ComposioCapabilitiesResponse`]. Describes the compiled module,
    /// not the user, so it answers without a session or a connection — which is
    /// what lets a caller tell "you cannot connect this" apart from "you can
    /// connect it, but nothing will read it yet".
    pub const LIST_CAPABILITIES: &str = "ListCapabilities";

    /// Lists the toolkits that ship a curated agent catalog.
    ///
    /// Takes no arguments and returns a
    /// [`crate::ComposioAgentReadyToolkitsResponse`]. A connected toolkit
    /// absent from this list should be surfaced as preview rather than as
    /// something the agent can already act through.
    pub const LIST_AGENT_READY_TOOLKITS: &str = "ListAgentReadyToolkits";

    /// Reads what the user has allowed an agent to do with a toolkit.
    ///
    /// Takes a [`crate::ComposioGetUserScopesRequest`] and returns a
    /// [`crate::ComposioUserScopesResponse`]. A toolkit with nothing stored
    /// reports the default rather than an absence, so a caller has no unset
    /// state to handle.
    pub const GET_USER_SCOPES: &str = "GetUserScopes";

    /// Writes what the user allows an agent to do with a toolkit.
    ///
    /// Takes a [`crate::ComposioSetUserScopesRequest`] and returns the stored
    /// [`crate::ComposioUserScopesResponse`].
    ///
    /// The preference is enforced by the module, not by its caller:
    /// [`LIST_TOOLS`] hides what it forbids and [`EXECUTE`] refuses it. A
    /// caller cannot opt out of a restriction the user set.
    pub const SET_USER_SCOPES: &str = "SetUserScopes";
}

/// Every member of [`INTERFACE`], in the order the interface dispatches them.
///
/// `crates/tinyconnectors` asserts its declared manifest methods against this
/// list, so the two cannot drift.
pub const METHODS: &[&str] = &[
    methods::LIST_TOOLKITS,
    methods::LIST_CONNECTIONS,
    methods::AUTHORIZE,
    methods::DELETE_CONNECTION,
    methods::LIST_TOOLS,
    methods::EXECUTE,
    methods::LIST_CAPABILITIES,
    methods::LIST_AGENT_READY_TOOLKITS,
    methods::GET_USER_PROFILE,
    methods::REFRESH_ALL_IDENTITIES,
    methods::LIST_GITHUB_REPOS,
    methods::LIST_AVAILABLE_TRIGGERS,
    methods::LIST_TRIGGERS,
    methods::CREATE_TRIGGER,
    methods::ENABLE_TRIGGER,
    methods::DISABLE_TRIGGER,
    methods::LIST_TRIGGER_HISTORY,
];

#[cfg(test)]
mod test;
