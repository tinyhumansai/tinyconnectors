//! Pulling records out of connected accounts, for a host to ingest.
//!
//! A sync run reads a user's Gmail, Slack, or Notion through a connector and
//! produces [`tinyconnectors_bus::ConnectorRecordBatch`]. It does not store
//! anything. The host takes the batch and writes it to memory over memory's own
//! bus API.
//!
//! # Why this crate has no memory dependency
//!
//! These pipelines used to live inside the memory system and call its store
//! directly, which is why they could not be moved without taking half of memory
//! with them. Returning records instead cuts that: a pipeline knows how to talk
//! to Gmail, memory knows how to store things, and neither links the other.
//!
//! The one thing a pipeline genuinely needs to remember between runs — where it
//! got to, what it has already seen, how much of today's request budget is left
//! — goes through [`state::SyncStateStore`], a small key-value seam the host
//! implements. That is deliberately not a memory dependency: it is two methods
//! over JSON, and a host can back it with anything.
//!
//! # What is here
//!
//! - [`scope`] — how invasive an action is, and the curated catalogs that keep
//!   a toolkit's sixty-odd actions from all reaching the agent.
//! - [`prefs`] — what the user has allowed an agent to do with each toolkit.
//! - [`pipeline`] — the loop around a provider's page: cursors, budgets, item
//!   limits, and dedupe.
//! - [`provider`] — what a connector knows about one toolkit, and the registry
//!   that looks one up by slug.
//! - [`state`] — per-connection cursors, dedupe sets, and the daily request
//!   budget, persisted through the host's key-value seam.
//! - [`toolkits`] — the toolkits this build knows, each with its curated action
//!   catalog and how to read the connected account's identity.
//!
//! # Example
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
pub mod pipeline;
pub mod prefs;
pub mod provider;
pub mod scope;
pub mod state;
pub mod toolkits;

pub use error::{Error, Result};
pub use pipeline::{ProviderPage, SyncOutcome, run_sync};
pub use prefs::{PREFS_NAMESPACE, UserScopePref};
pub use provider::{
    ActionRunner, ConnectorProvider, ProviderContext, ProviderRegistry, ProviderUserProfile,
    SyncLimits, SyncReason,
};
pub use scope::{CuratedTool, ToolScope, classify_unknown, find_curated, toolkit_from_slug};
pub use state::{DailyBudget, STATE_NAMESPACE, SyncState, SyncStateStore};
pub use toolkits::default_registry;
