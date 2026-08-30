//! OAuth connector integrations, as an installable `TinyBus` module.
//!
//! This crate owns the connector surface: linking a user's third-party accounts
//! over OAuth, listing what those accounts can do, running actions against
//! them, and subscribing to the webhooks they emit. The compiled `cdylib`
//! exports `TinyBus` module ABI v1 and serves that surface over the bus, so a
//! host — `openhuman`, `tinycortex` — gains connectors by loading a binary
//! rather than by compiling them in.
//!
//! # Backends
//!
//! Composio is the connector backend today. It is not assumed to be the only
//! one: its vocabulary is namespaced under [`tinyconnectors_bus::composio`], and
//! policy that is not Composio-specific — the OAuth handoff rules in [`oauth`],
//! for instance — is written without reference to it.
//!
//! # Layout
//!
//! This is the implementation half of a two-crate workspace:
//!
//! - [`tinyconnectors_bus`] — the wire contract. Member names, payload types,
//!   and the contract version, with no transport and no behavior. A host that
//!   only makes calls depends on that crate alone.
//! - `tinyconnectors` — this crate. The behavior, the crate-wide error type,
//!   and the `TinyBus` adapter that serves them, built as both an `rlib` and
//!   the `cdylib` the loader consumes.
//!
//! Within this crate:
//!
//! - `src/error/` holds the crate-wide [`Error`] enum and the [`Result`] alias
//!   returned by every fallible public function.
//! - `src/oauth/` holds the account-linking policy.
//! - `tinybus_module` adapts the public behavior to `TinyBus` and exports the
//!   module descriptor, embedded manifest, and initialization entrypoint.
//!
//! Every public item is re-exported from here — including all of
//! [`tinyconnectors_bus`] — so downstream users have a single predictable
//! surface and `tinyconnectors::ComposioConnection` is the *same type* as
//! `tinyconnectors_bus::ComposioConnection`, not a structural twin.
//!
//! # Example
//!
//! ```
//! use tinyconnectors::{ComposioConnection, oauth};
//!
//! // A handoff the user started but never finished leaves a row behind.
//! let abandoned: ComposioConnection = serde_json::from_value(serde_json::json!({
//!     "id": "conn_1", "toolkit": "instagram", "status": "PENDING",
//! }))?;
//! assert!(!abandoned.is_active());
//!
//! // Meta rate-limits an account that accumulates those, so a fresh handoff
//! // clears them first.
//! assert!(oauth::is_meta_oauth_toolkit(&abandoned.toolkit));
//! assert!(oauth::is_clearable_oauth_status(&abandoned.status));
//! # Ok::<(), serde_json::Error>(())
//! ```

mod error;
pub mod oauth;
mod tinybus_module;

pub use error::{Error, Result};

// The wire contract, re-exported by module rather than by item so every path
// through this crate resolves to the same definitions the contract crate
// publishes. A host may depend on `tinyconnectors-bus` directly and get exactly
// these types; nothing here redefines them.
pub use tinyconnectors_bus;
pub use tinyconnectors_bus::{
    CONTRACT_VERSION, ComposioActiveTrigger, ComposioActiveTriggersResponse,
    ComposioAgentReadyToolkitsResponse, ComposioAuthorizeResponse, ComposioAvailableTrigger,
    ComposioAvailableTriggerRepo, ComposioAvailableTriggersResponse, ComposioCapabilitiesResponse,
    ComposioCapability, ComposioConnection, ComposioConnectionsResponse,
    ComposioCreateTriggerResponse, ComposioDeleteResponse, ComposioDisableTriggerResponse,
    ComposioEnableTriggerResponse, ComposioExecuteResponse, ComposioGithubRepo,
    ComposioGithubReposResponse, ComposioToolFunction, ComposioToolSchema,
    ComposioToolkitCatalogEntry, ComposioToolkitsResponse, ComposioToolsResponse,
    ComposioTriggerEvent, ComposioTriggerHistoryEntry, ComposioTriggerHistoryResult,
    ComposioTriggerMetadata, INTERFACE, METHODS, OBJECT_PATH, composio, is_compatible, names,
    version,
};
