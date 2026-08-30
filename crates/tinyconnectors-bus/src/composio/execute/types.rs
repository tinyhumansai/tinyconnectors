//! Execute response payload.

use serde::{Deserialize, Serialize};

/// Arguments for running one action.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposioExecuteRequest {
    /// Action slug, e.g. `"GMAIL_SEND_EMAIL"`.
    pub tool: String,
    /// Action arguments. Must be a JSON object when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
    /// Connection to run against.
    ///
    /// `None` uses the ambient account for the toolkit, which is only
    /// unambiguous when the user has exactly one. Name the connection whenever
    /// the caller knows which it means.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
}

/// Response body of `POST /agent-integrations/composio/execute`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposioExecuteResponse {
    /// Raw result from the upstream provider.
    #[serde(default)]
    pub data: serde_json::Value,
    /// Whether the provider reported success.
    ///
    /// Independent of transport success: a call can return HTTP 200 carrying
    /// `successful: false` and an `error`, and treating that as a win is the
    /// classic way to report a send that never happened.
    #[serde(default)]
    pub successful: bool,
    /// Provider error message, when there is one.
    #[serde(default)]
    pub error: Option<String>,
    /// Amount charged to the caller, base plus margin, in USD.
    #[serde(rename = "costUsd", default)]
    pub cost_usd: f64,
    /// Backend-rendered compact markdown for known tools.
    ///
    /// When present and non-empty a caller should prefer this over `data` for
    /// model and CLI consumption — it is far cheaper in tokens than the raw
    /// provider envelope.
    #[serde(rename = "markdownFormatted", default)]
    pub markdown_formatted: Option<String>,
}
