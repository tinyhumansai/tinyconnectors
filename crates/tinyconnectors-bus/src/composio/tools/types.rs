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

/// Response body of `GET /agent-integrations/composio/tools`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposioToolsResponse {
    /// Tools available for the requested toolkit.
    #[serde(default)]
    pub tools: Vec<ComposioToolSchema>,
}
