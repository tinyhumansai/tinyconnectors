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
//! [`Transport`] is the seam instead: the host implements three verbs, and
//! [`ComposioClient`] spells the paths and parses the envelopes on top. A test
//! implements the same three verbs over a fixture map, which is why the client
//! tests below need no network.
//!
//! # What the client does not do
//!
//! It does not retry, and it does not interpret failures. Rate-limit backoff is
//! OAuth handoff policy and lives in [`crate::oauth`]; layering a second retry
//! here would multiply the two.

mod composio;
mod http;
mod transport;

pub use composio::ComposioClient;
pub use http::HttpTransport;
pub use transport::Transport;

#[cfg(test)]
mod test;
