//! Connection, authorize, and delete payloads.

use serde::{Deserialize, Serialize};

/// One connected account — an OAuth integration instance for a toolkit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposioConnection {
    /// Connection id. This is what a caller deletes to disconnect.
    pub id: String,
    /// Toolkit slug, e.g. `"gmail"`.
    pub toolkit: String,
    /// Connection status — `"ACTIVE"`, `"CONNECTED"`, `"PENDING"`, …
    ///
    /// Free-form on purpose: the backend passes it through from Composio, so
    /// an unrecognized spelling must survive the round trip rather than be
    /// mapped onto an enum that would reject it.
    pub status: String,
    /// ISO timestamp, passed through from Composio.
    #[serde(rename = "createdAt", default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Account email, when the toolkit reports one (Gmail, Google Calendar,
    /// Google Sheets). Lets a picker show `Gmail · user@example.com`.
    #[serde(
        rename = "accountEmail",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub account_email: Option<String>,
    /// Workspace or team display name, for workspace-based services (a Slack
    /// team, a Notion workspace). Used when no email is available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// Screen name or handle, for username-based services (a GitHub login).
    /// The last-resort identity hint after email and workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
}

impl ComposioConnection {
    /// The toolkit slug in the canonical form used by provider lookup, prompt
    /// injection, and tool-action prefix matching.
    #[must_use]
    pub fn normalized_toolkit(&self) -> String {
        self.toolkit.trim().to_ascii_lowercase()
    }

    /// Whether this row represents a usable connection.
    ///
    /// Status is compared case-insensitively because the web UI already does.
    /// Keeping the two aligned is what stops a backend spelling of `connected`
    /// from displaying as connected in Settings while disappearing from the
    /// agent's integration surface.
    #[must_use]
    pub fn is_active(&self) -> bool {
        let status = self.status.trim();
        status.eq_ignore_ascii_case("ACTIVE") || status.eq_ignore_ascii_case("CONNECTED")
    }
}

/// Response body of `GET /agent-integrations/composio/connections`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposioConnectionsResponse {
    /// Every connection row for the user, active or not.
    #[serde(default)]
    pub connections: Vec<ComposioConnection>,
}

/// Arguments for beginning an OAuth handoff.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposioAuthorizeRequest {
    /// Toolkit slug to authorize. Must be on the backend allowlist.
    pub toolkit: String,
    /// Extra body fields for toolkits that need them — a WhatsApp Business
    /// account id, for instance.
    ///
    /// Keys the backend derives itself are refused rather than merged: a value
    /// that arrived over the bus must not be able to redirect the handoff at a
    /// different toolkit or credential.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra_params: Option<serde_json::Value>,
}

/// Arguments for disconnecting a connection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposioDeleteConnectionRequest {
    /// Id of the connection to remove.
    pub connection_id: String,
    /// Whether to delete memory sourced from that connection along with it.
    #[serde(default)]
    pub clear_memory: bool,
}

/// Response body of `POST /agent-integrations/composio/authorize`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposioAuthorizeResponse {
    /// Composio-hosted OAuth URL the user opens in a browser.
    #[serde(rename = "connectUrl")]
    pub connect_url: String,
    /// Id of the connection row this authorize call created.
    #[serde(rename = "connectionId")]
    pub connection_id: String,
}

/// Response body of `DELETE /agent-integrations/composio/connections/:id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposioDeleteResponse {
    /// Whether the connection row was removed.
    #[serde(default)]
    pub deleted: bool,
    /// How many memory chunks sourced from that connection were removed with
    /// it. Disconnecting an account must not leave its content behind.
    #[serde(default)]
    pub memory_chunks_deleted: usize,
}
