//! Tool schema payloads.

use serde::{Deserialize, Serialize};

/// One tool, in the function-calling envelope a model expects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposioToolSchema {
    /// Envelope discriminator. Always `"function"` in practice; defaulted so a
    /// backend that omits it still parses.
    #[serde(rename = "type", default = "default_function_type")]
    pub kind: String,
    /// The callable itself.
    pub function: ComposioToolFunction,
}

fn default_function_type() -> String {
    "function".to_string()
}

/// The name, description, and schemas of one callable action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposioToolFunction {
    /// Action slug, e.g. `"GMAIL_SEND_EMAIL"`.
    pub name: String,
    /// Human-readable description shown to the model.
    #[serde(default)]
    pub description: Option<String>,
    /// JSON schema for the action's input parameters.
    #[serde(default)]
    pub parameters: Option<serde_json::Value>,
    /// JSON schema for the action's return value, when the upstream listing
    /// publishes one.
    ///
    /// `None` means *unknown*, not *empty*: the backend-proxied `/tools` path
    /// is opaque to this crate and may not forward the field, and not every
    /// action publishes an output schema in the first place.
    #[serde(default)]
    pub output_parameters: Option<serde_json::Value>,
}

/// Which scopes an agent may use for one toolkit.
///
/// Three independent flags rather than one maximum level: a user who wants an
/// agent to read and delete stale mail but never send any is expressing
/// something a threshold cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposioUserScopes {
    /// Whether the agent may read.
    #[serde(default = "enabled")]
    pub read: bool,
    /// Whether the agent may create or change things.
    #[serde(default = "enabled")]
    pub write: bool,
    /// Whether the agent may delete or change permissions.
    #[serde(default)]
    pub admin: bool,
}

fn enabled() -> bool {
    true
}

impl Default for ComposioUserScopes {
    /// Read and write, but not admin.
    ///
    /// Read alone makes most integrations useless — an agent that can see a
    /// calendar but not add to it is a worse version of looking yourself. Admin
    /// is off because its actions destroy things.
    fn default() -> Self {
        Self {
            read: true,
            write: true,
            admin: false,
        }
    }
}

/// Arguments for reading a toolkit's scope preference.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposioGetUserScopesRequest {
    /// Toolkit whose preference to read.
    pub toolkit: String,
}

/// Arguments for writing a toolkit's scope preference.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposioSetUserScopesRequest {
    /// Toolkit whose preference to write.
    pub toolkit: String,
    /// The preference to store. Absent flags take their default.
    #[serde(flatten)]
    pub scopes: ComposioUserScopes,
}

/// A toolkit's scope preference.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposioUserScopesResponse {
    /// Toolkit the preference is for.
    pub toolkit: String,
    /// The stored preference, or the default when none is stored.
    #[serde(flatten)]
    pub scopes: ComposioUserScopes,
}

/// Arguments for listing a toolkit's tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposioListToolsRequest {
    /// Toolkit slugs to list tools for. Empty means every connected toolkit,
    /// which is a large answer — callers normally name what they need.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub toolkits: Vec<String>,
    /// Composio action tags to filter by.
    ///
    /// OR semantics: more tags broaden the result rather than narrowing it,
    /// which is the opposite of what the word "filter" suggests.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Whether to hide actions the user's scope preference forbids.
    ///
    /// On by default, because a listing is what an agent picks from: showing it
    /// an action it will then be refused wastes a turn and reads to the model
    /// as a malfunction. Set false to see the full catalog — a settings screen
    /// rendering the choices, rather than an agent about to act.
    #[serde(default = "enabled")]
    pub apply_user_scopes: bool,
}

impl Default for ComposioListToolsRequest {
    fn default() -> Self {
        Self {
            toolkits: Vec::new(),
            tags: Vec::new(),
            apply_user_scopes: true,
        }
    }
}

/// Response body of `GET /agent-integrations/composio/tools`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposioToolsResponse {
    /// Tools available for the requested toolkit.
    #[serde(default)]
    pub tools: Vec<ComposioToolSchema>,
}
