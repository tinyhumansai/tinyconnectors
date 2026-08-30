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
pub mod methods {
    /// Lists the toolkits the backend allowlist currently enables.
    ///
    /// Returns a [`crate::ComposioToolkitsResponse`].
    pub const LIST_TOOLKITS: &str = "ListToolkits";

    /// Lists this build's connector capability matrix.
    ///
    /// Needs no signed-in session — it describes the compiled module. Returns a
    /// [`crate::ComposioCapabilitiesResponse`].
    pub const LIST_CAPABILITIES: &str = "ListCapabilities";

    /// Lists the toolkit slugs that ship a curated agent catalog.
    ///
    /// Returns a [`crate::ComposioAgentReadyToolkitsResponse`].
    pub const LIST_AGENT_READY_TOOLKITS: &str = "ListAgentReadyToolkits";

    /// Lists the caller's connections.
    ///
    /// Returns a [`crate::ComposioConnectionsResponse`].
    pub const LIST_CONNECTIONS: &str = "ListConnections";

    /// Begins an OAuth handoff for a toolkit.
    ///
    /// Returns a [`crate::ComposioAuthorizeResponse`] carrying the hosted
    /// connect URL the user opens in a browser.
    pub const AUTHORIZE: &str = "Authorize";

    /// Disconnects a connection, optionally clearing memory sourced from it.
    ///
    /// Returns a [`crate::ComposioDeleteResponse`].
    pub const DELETE_CONNECTION: &str = "DeleteConnection";

    /// Lists the callable tools for a set of toolkits.
    ///
    /// Returns a [`crate::ComposioToolsResponse`].
    pub const LIST_TOOLS: &str = "ListTools";

    /// Runs one action against a connected account.
    ///
    /// Returns a [`crate::ComposioExecuteResponse`].
    pub const EXECUTE: &str = "Execute";

    /// Lists the repositories a connected GitHub account can see.
    ///
    /// Returns a [`crate::ComposioGithubReposResponse`].
    pub const LIST_GITHUB_REPOS: &str = "ListGithubRepos";

    /// Creates a trigger subscription.
    ///
    /// Returns a [`crate::ComposioCreateTriggerResponse`].
    pub const CREATE_TRIGGER: &str = "CreateTrigger";

    /// Fetches the cached provider profile for a connection.
    pub const GET_USER_PROFILE: &str = "GetUserProfile";

    /// Re-fetches every connection's provider identity.
    pub const REFRESH_ALL_IDENTITIES: &str = "RefreshAllIdentities";

    /// Runs a sync for a connection.
    pub const SYNC: &str = "Sync";

    /// Reads recent trigger deliveries from the archive.
    ///
    /// Returns a [`crate::ComposioTriggerHistoryResult`].
    pub const LIST_TRIGGER_HISTORY: &str = "ListTriggerHistory";

    /// Reads the caller's per-toolkit scope preferences.
    pub const GET_USER_SCOPES: &str = "GetUserScopes";

    /// Writes the caller's per-toolkit scope preferences.
    pub const SET_USER_SCOPES: &str = "SetUserScopes";

    /// Lists the triggers a toolkit offers.
    ///
    /// Returns a [`crate::ComposioAvailableTriggersResponse`].
    pub const LIST_AVAILABLE_TRIGGERS: &str = "ListAvailableTriggers";

    /// Lists the caller's enabled trigger subscriptions.
    ///
    /// Returns a [`crate::ComposioActiveTriggersResponse`].
    pub const LIST_TRIGGERS: &str = "ListTriggers";

    /// Enables a trigger subscription.
    ///
    /// Returns a [`crate::ComposioEnableTriggerResponse`].
    pub const ENABLE_TRIGGER: &str = "EnableTrigger";

    /// Disables a trigger subscription.
    ///
    /// Returns a [`crate::ComposioDisableTriggerResponse`].
    pub const DISABLE_TRIGGER: &str = "DisableTrigger";

    /// Reports whether the module is proxying through the backend or calling
    /// Composio directly with a caller-supplied key.
    pub const GET_MODE: &str = "GetMode";

    /// Stores a direct-mode Composio API key.
    pub const SET_API_KEY: &str = "SetApiKey";

    /// Clears the stored direct-mode API key, returning to backend-proxied mode.
    pub const CLEAR_API_KEY: &str = "ClearApiKey";
}

/// Every member of [`INTERFACE`], in the order the interface dispatches them.
///
/// `crates/tinyconnectors` asserts its declared manifest methods against this
/// list, so the two cannot drift.
pub const METHODS: &[&str] = &[
    methods::LIST_TOOLKITS,
    methods::LIST_CAPABILITIES,
    methods::LIST_AGENT_READY_TOOLKITS,
    methods::LIST_CONNECTIONS,
    methods::AUTHORIZE,
    methods::DELETE_CONNECTION,
    methods::LIST_TOOLS,
    methods::EXECUTE,
    methods::LIST_GITHUB_REPOS,
    methods::CREATE_TRIGGER,
    methods::GET_USER_PROFILE,
    methods::REFRESH_ALL_IDENTITIES,
    methods::SYNC,
    methods::LIST_TRIGGER_HISTORY,
    methods::GET_USER_SCOPES,
    methods::SET_USER_SCOPES,
    methods::LIST_AVAILABLE_TRIGGERS,
    methods::LIST_TRIGGERS,
    methods::ENABLE_TRIGGER,
    methods::DISABLE_TRIGGER,
    methods::GET_MODE,
    methods::SET_API_KEY,
    methods::CLEAR_API_KEY,
];

#[cfg(test)]
mod test;
