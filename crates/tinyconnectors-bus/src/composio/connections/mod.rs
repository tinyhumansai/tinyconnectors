//! Connected accounts and the OAuth handoff that creates them.
//!
//! A connection is one linked account for one toolkit — a user may hold several
//! for the same toolkit (two Gmail addresses, two Slack workspaces), which is
//! why [`ComposioConnection`] carries identity hints the UI uses to tell them
//! apart rather than labelling them "Account 1" and "Account 2".
//!
//! # The OAuth handoff
//!
//! Authorizing returns a [`ComposioAuthorizeResponse`]: a Composio-hosted URL
//! the user opens in a browser, and the id of the row that URL will activate.
//! The row exists in a non-active state from that moment, so an abandoned or
//! retried handoff leaves rows behind — [`ComposioConnection::is_active`] is
//! what separates a usable connection from that debris.

mod types;

pub use types::{
    ComposioAuthorizeResponse, ComposioConnection, ComposioConnectionsResponse,
    ComposioDeleteResponse,
};

#[cfg(test)]
mod test;
