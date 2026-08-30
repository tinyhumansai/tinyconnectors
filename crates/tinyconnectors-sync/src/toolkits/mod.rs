//! The toolkits this build knows how to read.
//!
//! One module per toolkit, each holding a [`crate::ConnectorProvider`] and its
//! curated catalog. A provider is small on purpose: the slug, a description, how
//! often to re-read, which actions are worth offering, and how to read the
//! account's identity. Everything else is shared.
//!
//! # The catalogs are hand-picked, and that is the point
//!
//! Composio publishes sixty-odd actions for a typical toolkit. Offering all of
//! them makes a model's tool list worse, not better: the long tail is
//! edge-case administration nothing plans for, and every entry competes for the
//! model's attention with the handful that matter. Each catalog here is the
//! slice worth surfacing, ported action-for-action from the lists these
//! toolkits were already curated against.
//!
//! # Identity, not sync
//!
//! These providers read profiles. `fetch_records` still takes the trait's
//! default — no records — because the per-toolkit sync pipelines are the next
//! part of the migration. A toolkit therefore reports `initial_sync: false` in
//! the capability matrix until its pipeline lands, which is honest: nothing
//! will read it yet.

mod clickup;
mod clickup_catalog;
mod github;
mod github_catalog;
mod gmail;
mod gmail_catalog;
mod identity;
mod linear;
mod linear_catalog;
mod notion;
mod notion_catalog;

pub use clickup::ClickupProvider;
pub use github::GithubProvider;
pub use gmail::GmailProvider;
pub use linear::LinearProvider;
pub use notion::NotionProvider;

use std::sync::Arc;

use crate::provider::ProviderRegistry;

/// Every toolkit this build ships.
#[must_use]
pub fn default_registry() -> ProviderRegistry {
    ProviderRegistry::new()
        .with(Arc::new(ClickupProvider))
        .with(Arc::new(GithubProvider))
        .with(Arc::new(GmailProvider))
        .with(Arc::new(LinearProvider))
        .with(Arc::new(NotionProvider))
}

#[cfg(test)]
mod test;
