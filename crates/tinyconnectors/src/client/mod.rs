//! Talking to a connector backend, over a transport the host supplies.
//!
//! # Why the transport is a trait
//!
//! The module does not hold the Composio API key and does not call Composio.
//! Every request goes through a backend that owns the key, the billing margin,
//! the toolkit allowlist, and the HMAC verification of inbound webhooks. That
//! backend authenticates the *user*, which means the credential is the host's,
//! acquired the host's way — a Bearer JWT here, something else in the next host.
//!
//! Baking one host's auth into this crate would make it that host's library.
//! [`Transport`] is the seam instead: the host implements three verbs, and the
//! layers above spell the paths and parse the envelopes. A test implements the
//! same three verbs over a fixture map, which is why the client tests need no
//! network.
//!
//! # Two routes, one client
//!
//! Composio is reachable two ways — proxied through the `TinyHumans` backend, or
//! directly with a user-supplied API key — and they differ in base URL, auth
//! header, paths, *and* response shape. [`Route`] absorbs all four differences;
//! [`ComposioClient`] holds the policy that is the same either way and calls
//! through it. Nothing above this module branches on which route is live.
//!
//! Selecting a route is host policy, stated in the module configuration blob.
//! See [`route`] for why the two are not equivalent.
//!
//! # What the client does not do
//!
//! It does not retry, and it does not interpret failures. Rate-limit backoff is
//! OAuth handoff policy and lives in [`crate::oauth`]; layering a second retry
//! here would multiply the two.

mod composio;
mod http;
pub mod route;
mod transport;

pub use composio::ComposioClient;
pub use http::HttpTransport;
pub use route::{COMPOSIO_API_BASE, DirectRoute, INVALID_API_KEY_THRESHOLD, ProxyRoute, Route};
pub use transport::Transport;

#[cfg(test)]
mod test;
