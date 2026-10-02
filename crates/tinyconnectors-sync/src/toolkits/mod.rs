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
//! # Flat toolkits, and one that is not
//!
//! Five of these are *flat*: one action reads the whole account, one cursor
//! says where it got to, and the provider is a
//! [`PageSpec`](crate::pipeline::PageSpec) declaration and little else.
//!
//! Slack is *scoped*. Its history action reads one channel and requires the
//! channel's id, so a sync walks the conversation list and pages each channel
//! with a position of its own. It writes its own `fetch_page` and carries that
//! position in a composite cursor, which the run loop stores without looking
//! inside — see [`SlackProvider`] for why that was the right seam and what
//! it avoided changing.
//!
//! Every toolkit registered here can be read. The capability matrix says so —
//! `initial_sync` and `periodic_sync` are both derived from
//! [`crate::ConnectorProvider::can_sync`], and a test asserts them for every
//! row — so registering a provider that cannot sync would make the matrix
//! promise a user something no build can deliver.

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
mod slack;
mod slack_catalog;
mod slack_files;
mod slack_parse;

pub use clickup::ClickupProvider;
pub use github::GithubProvider;
pub use gmail::GmailProvider;
pub use linear::LinearProvider;
pub use notion::NotionProvider;
pub use slack::SlackProvider;

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
        .with(Arc::new(SlackProvider))
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod test;
