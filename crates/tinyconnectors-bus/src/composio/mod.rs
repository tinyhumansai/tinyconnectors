//! The Composio backend's wire vocabulary.
//!
//! Composio is one OAuth connector backend, not the only one this contract
//! expects to carry. Everything Composio-shaped lives under this module so a
//! second backend — a direct OAuth broker, a self-hosted equivalent — arrives
//! as a sibling module rather than as a rename of every type in the crate.
//!
//! # What is here
//!
//! One directory per payload family, each mirroring a group of backend
//! response envelopes:
//!
//! - [`toolkits`] — the connectable catalog and this build's capability matrix.
//! - [`connections`] — connected accounts, the OAuth authorize handoff, delete.
//! - [`tools`] — function-calling schemas for a toolkit's actions.
//! - [`execute`] — the result of running one action.
//! - [`triggers`] — the trigger catalog, active subscriptions, and the webhook
//!   events they deliver.
//! - [`github`] — the repository listing GitHub-scoped triggers are bound to.
//!
//! # These shapes are not ours to choose
//!
//! Every type here mirrors a response envelope emitted by the `OpenHuman`
//! backend under `/agent-integrations/composio/*`, which in turn forwards
//! Composio's own shapes. Field names and `#[serde(...)]` attributes are a wire
//! contract: a host and a module that disagree about a field name fail at
//! runtime with a decode error, not at compile time. Each family pins its serde
//! representation in `test.rs` for exactly that reason.
//!
//! The `Composio` prefix on the type names is deliberate and stays. These are
//! Composio's envelopes, and naming them as if they were a neutral abstraction
//! would be a lie the moment a second backend disagreed about a field.

pub mod connections;
pub mod execute;
pub mod github;
pub mod toolkits;
pub mod tools;
pub mod triggers;

mod serde_compat;

pub use connections::{
    ComposioAuthorizeRequest, ComposioAuthorizeResponse, ComposioConnection,
    ComposioConnectionsResponse, ComposioDeleteConnectionRequest, ComposioDeleteResponse,
};
pub use execute::{ComposioExecuteRequest, ComposioExecuteResponse};
pub use github::{
    ComposioGithubRepo, ComposioGithubReposResponse, ComposioListGithubReposRequest,
};
pub use toolkits::{
    ComposioAgentReadyToolkitsResponse, ComposioCapabilitiesResponse, ComposioCapability,
    ComposioToolkitCatalogEntry, ComposioToolkitsResponse,
};
pub use tools::{
    ComposioListToolsRequest, ComposioToolFunction, ComposioToolSchema, ComposioToolsResponse,
};
pub use triggers::{
    ComposioActiveTrigger, ComposioActiveTriggersResponse, ComposioAvailableTrigger,
    ComposioAvailableTriggerRepo, ComposioAvailableTriggersResponse, ComposioCreateTriggerResponse,
    ComposioDisableTriggerResponse, ComposioEnableTriggerResponse, ComposioTriggerEvent,
    ComposioTriggerHistoryEntry, ComposioTriggerHistoryResult, ComposioTriggerMetadata,
};
