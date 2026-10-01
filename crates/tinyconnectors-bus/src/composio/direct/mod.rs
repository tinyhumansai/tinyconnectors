//! Stateless direct reads, and the network settings a host hands the module.
//!
//! # Why this family exists
//!
//! The module holds exactly one route at a time, installed by `Configure`. A
//! host that serves several credentials from one process — a signed-in user's
//! stored key, an embedded agent pinned to its own key, a candidate key typed
//! into a settings form and not yet saved — cannot share that route without the
//! callers reconfiguring each other mid-call.
//!
//! [`ComposioDirectConnectionsRequest`] and [`ComposioDirectToolsRequest`] carry
//! their own [`ComposioDirectCredential`] instead, so the member answers with
//! that credential and touches nothing else: the configured route is not read
//! or replaced, nothing is persisted, and the key does not outlive the call.
//! That is also how an unsaved key is validated before it is stored.
//!
//! # Network settings
//!
//! [`ComposioTransportConfig`] is how the host's proxy and TLS policy reaches the
//! module's HTTP client. It is part of the route description (`Configure`, the
//! load-time blob) and of every direct credential, so the same policy applies to
//! a configured route and to a one-off read.

mod types;

pub use types::{
    ComposioDirectConnectionsRequest, ComposioDirectCredential, ComposioDirectToolsRequest,
    ComposioTlsRoots, ComposioTransportConfig,
};

#[cfg(test)]
#[path = "mod_tests.rs"]
mod test;
