//! Wiring the provider registry to this module's client.
//!
//! [`tinyconnectors_sync`] describes what a connector knows about a toolkit but
//! deliberately cannot reach Composio — it has no transport and takes no
//! dependency on one. [`ClientActions`] closes that: it adapts this module's
//! [`ComposioClient`] to the [`ActionRunner`] seam a provider calls through.
//!
//! # Why a provider's failures are stricter than a member's
//!
//! `Execute` reports a refused action as a successful reply carrying
//! `successful: false`, because a caller asked to run something and deserves to
//! know exactly what the provider said. A *provider* reading a page of a user's
//! mailbox has nothing useful to do with a half-answer, so the same refusal
//! becomes an error here — the sync stops rather than recording an empty page
//! as a complete one.

mod actions;

pub use actions::ClientActions;

use tinyconnectors_sync::ProviderRegistry;

/// The providers this build ships.
///
/// Empty while the concrete toolkit providers are still being migrated. That is
/// visible rather than hidden: the capability matrix reports nothing, and
/// `GetUserProfile` says which toolkit it has no provider for, instead of a
/// member quietly returning a plausible blank.
#[must_use]
pub fn default_registry() -> ProviderRegistry {
    ProviderRegistry::new()
}

#[cfg(test)]
mod test;
