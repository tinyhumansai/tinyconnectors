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
];

#[cfg(test)]
mod test;
