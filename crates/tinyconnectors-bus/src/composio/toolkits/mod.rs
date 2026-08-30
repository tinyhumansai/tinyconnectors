//! What a user can connect, and what this build can do once they have.
//!
//! Two different questions live here and they are easy to confuse:
//!
//! - [`ComposioToolkitsResponse`] answers *"what may this user connect?"* — a
//!   server-enforced allowlist plus optional render metadata from the live
//!   Composio catalog. It depends on a signed-in backend session.
//! - [`ComposioCapabilitiesResponse`] answers *"what does this build know how
//!   to do with a toolkit once connected?"* — native provider, curated tools,
//!   sync hooks, memory ingestion. It is a property of the compiled binary and
//!   needs no session at all.
//!
//! A toolkit can be connectable with no capabilities (the user links it, but
//! the agent has no curated actions for it yet), which is what
//! [`ComposioAgentReadyToolkitsResponse`] exists to let a UI label honestly.

mod types;

pub use types::{
    ComposioAgentReadyToolkitsResponse, ComposioCapabilitiesResponse, ComposioCapability,
    ComposioToolkitCatalogEntry, ComposioToolkitsResponse,
};

#[cfg(test)]
mod test;
