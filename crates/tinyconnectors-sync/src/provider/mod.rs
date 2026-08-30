//! What a connector knows about one toolkit.
//!
//! A provider is the toolkit-specific half of a sync: which action reads the
//! user's profile, which actions are worth offering an agent, how often to
//! re-read, and how to turn a page of provider JSON into records. Everything
//! else — the transport, the retry policy, the record vocabulary — is shared.
//!
//! # Providers do not store anything
//!
//! A provider returns [`tinyconnectors_bus::ConnectorRecord`]s. It does not
//! reach a memory store, and there is nothing in [`ProviderContext`] that would
//! let it: the context carries the connection it is syncing, the limits it must
//! respect, and a way to call actions. That is the whole of it.
//!
//! # Calling actions is a seam too
//!
//! A provider needs to run Composio actions, but this crate must not depend on
//! the module that knows how. [`ActionRunner`] is the seam: one method, taking
//! a slug and arguments and returning the provider's JSON. The module supplies
//! an implementation backed by its client; a test supplies one backed by
//! fixtures, which is why every provider here is testable without a network.

mod context;
mod registry;
mod traits;

pub use context::{ActionRunner, ProviderContext, SyncLimits};
pub use registry::ProviderRegistry;
pub use traits::{ConnectorProvider, ProviderUserProfile, SyncReason};

#[cfg(test)]
mod test;
