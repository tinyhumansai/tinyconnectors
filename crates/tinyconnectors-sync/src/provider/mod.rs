//! What a connector knows about one toolkit.
//!
//! A provider is the toolkit-specific half of a connection: which action reads
//! the user's profile, and which actions are worth offering an agent. Everything
//! else — the transport, the retry policy, scope gating — is shared.
//!
//! # Calling actions is a seam
//!
//! A provider needs to run Composio actions, but this crate must not depend on
//! the module that knows how. [`ActionRunner`] is the seam: one method, taking
//! a slug and arguments and returning the provider's JSON. The module supplies
//! an implementation backed by its client; a test supplies one backed by
//! fixtures, which is why every provider here is testable without a network.

mod context;
mod registry;
mod traits;

pub use context::{ActionRunner, ProviderContext};
pub use registry::ProviderRegistry;
pub use traits::{ConnectorProvider, ProviderUserProfile};

#[cfg(test)]
#[path = "mod_tests.rs"]
mod test;
