//! The provider registry, scope catalogs and preferences behind the connector
//! module.
//!
//! A provider is what a connector knows about one toolkit: how to read the
//! connected account's identity and which of the toolkit's actions are worth
//! offering an agent. The module crate answers its capability, profile and
//! scope members from the registry in this crate. Nothing here pulls records
//! out of an account on a schedule, and nothing here stores anything: a
//! preference goes through [`prefs::PrefsStore`], a small key-value seam the
//! host implements.
//!
//! # What is here
//!
//! - [`scope`] — how invasive an action is, and the curated catalogs that keep
//!   a toolkit's sixty-odd actions from all reaching the agent.
//! - [`prefs`] — what the user has allowed an agent to do with each toolkit,
//!   and the [`PrefsStore`] it is persisted through.
//! - [`provider`] — what a connector knows about one toolkit, and the registry
//!   that looks one up by slug.
//! - [`toolkits`] — the toolkits this build knows, each with its curated action
//!   catalog and how to read the connected account's identity.
//!
# Example
//!
//! ```
//! use tinyconnectors_sync::scope::{ToolScope, classify_unknown, toolkit_from_slug};
//!
//! // An uncurated action is classified by its verb, so a destructive one is
//! // never surfaced as a harmless read.
//! assert_eq!(classify_unknown("GMAIL_TRASH_EMAIL"), ToolScope::Admin);
//! assert_eq!(classify_unknown("GMAIL_FETCH_EMAILS"), ToolScope::Read);
//!
//! assert_eq!(toolkit_from_slug("GMAIL_SEND_EMAIL").as_deref(), Some("gmail"));
//! ```

mod error;
pub mod prefs;
pub mod provider;
pub mod scope;
pub mod toolkits;

pub use error::{Error, Result};
pub use prefs::{PREFS_NAMESPACE, PrefsStore, UserScopePref};
pub use provider::{
    ActionRunner, ConnectorProvider, ProviderContext, ProviderRegistry, ProviderUserProfile,
};
pub use scope::{CuratedTool, ToolScope, classify_unknown, find_curated, toolkit_from_slug};
pub use toolkits::default_registry;
