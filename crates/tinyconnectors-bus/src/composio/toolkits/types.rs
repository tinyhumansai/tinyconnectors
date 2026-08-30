//! Toolkit catalog and capability payloads.

use serde::{Deserialize, Serialize};

/// One toolkit from the live Composio catalog, forwarded verbatim.
///
/// The module does not interpret these fields — it passes them straight to the
/// desktop UI so the app does not hardcode toolkit display metadata. Everything
/// except `slug` is best-effort: a backend predating the dynamic catalog omits
/// the whole `catalog` array, and the UI falls back to local metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposioToolkitCatalogEntry {
    /// Toolkit slug as Composio emits it, e.g. `"googlecalendar"`.
    pub slug: String,
    /// Human-readable name, e.g. `"Google Calendar"`.
    #[serde(default)]
    pub name: String,
    /// Composio-hosted logo URL (`meta.logo`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo: Option<String>,
    /// Short description (`meta.description`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Composio category names (`meta.categories`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    /// Whether the user may connect this toolkit — that is, whether it passed
    /// the backend's allowlist gate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Response body of `GET /agent-integrations/composio/toolkits`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposioToolkitsResponse {
    /// Server-enforced toolkit allowlist, e.g. `["gmail", "notion"]`.
    #[serde(default)]
    pub toolkits: Vec<String>,
    /// Rich render model from the live Composio catalog. Empty when the backend
    /// predates the dynamic catalog; forwarded as-is to the UI otherwise.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub catalog: Vec<ComposioToolkitCatalogEntry>,
}

/// One row of this build's connector capability matrix.
///
/// Unlike [`ComposioToolkitsResponse`] this is not tied to a signed-in session.
/// It describes what the compiled module knows how to do for a toolkit, so a UI
/// can distinguish "you cannot connect this" from "you can connect it but
/// nothing will read it yet".
// Seven bools is a lot, and `clippy::struct_excessive_bools` is right to say
// so about a type someone designed. This one is a wire shape: each flag is a
// separate field a UI reads by name, and folding them into a bitflag or a
// nested enum would change the JSON every consumer already parses.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposioCapability {
    /// Toolkit slug the row describes, e.g. `"gmail"`.
    pub toolkit: String,
    /// One-line summary of what connecting this toolkit gets the user.
    pub description: String,
    /// Whether a native provider implementation exists for the toolkit.
    pub native_provider: bool,
    /// Whether the module ships a curated agent tool catalog for it.
    pub curated_tools: bool,
    /// How many curated tools that catalog holds.
    pub curated_tool_count: usize,
    /// Whether arbitrary actions can be executed against the toolkit.
    pub tool_execution: bool,
    /// Whether a user profile can be fetched from the connected account.
    pub user_profile: bool,
    /// Whether a first, backfilling sync runs on connect.
    pub initial_sync: bool,
    /// Whether the toolkit is re-synced on a schedule.
    pub periodic_sync: bool,
    /// Interval between periodic syncs, when one is scheduled.
    pub sync_interval_secs: Option<u64>,
    /// Whether the toolkit delivers webhook-backed triggers.
    pub trigger_webhooks: bool,
    /// Whether synced records are written into memory.
    pub memory_ingest: bool,
}

/// Response body of the capability-matrix member.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposioCapabilitiesResponse {
    /// One row per toolkit this build knows about.
    #[serde(default)]
    pub capabilities: Vec<ComposioCapability>,
}

/// Sorted slugs that have a curated agent catalog.
///
/// A frontend uses this to decide whether to label a connected toolkit as
/// "preview — agent integration coming soon" rather than presenting it as
/// something the agent can already act through.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposioAgentReadyToolkitsResponse {
    /// Agent-ready toolkit slugs, sorted.
    #[serde(default)]
    pub toolkits: Vec<String>,
}
