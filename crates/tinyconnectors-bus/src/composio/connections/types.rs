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
    /// Extra body fields for toolkits that need them — a `WhatsApp` Business
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

/// Arguments for reading a connected account's identity.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposioUserProfileRequest {
    /// Toolkit whose provider knows how to read the profile.
    pub toolkit: String,
    /// Connection to read. `None` uses the toolkit's ambient account.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
}

/// A connected account's identity, as far as its toolkit reports it.
///
/// Every field is optional because the toolkits disagree about which they have:
/// Gmail knows an email, Slack a workspace and display name, `GitHub` a login.
/// A caller picking a label falls back through them in that order.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposioUserProfile {
    /// Toolkit the profile is for.
    pub toolkit: String,
    /// Connection the profile was read through.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    /// Human name, when the provider reports one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Account email.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Login or handle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Avatar image URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    /// Link to the account on the provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_url: Option<String>,
    /// Anything toolkit-specific, so a new toolkit's interesting field does not
    /// require widening this shape and every consumer of it.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub extras: serde_json::Value,
}

/// Response body of the identity-refresh member.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposioRefreshIdentitiesResponse {
    /// One profile per connection that could be read.
    ///
    /// A connection whose profile could not be read is simply absent: a refresh
    /// that failed for one account must still report the others, or one broken
    /// connection hides every working one.
    #[serde(default)]
    pub profiles: Vec<ComposioUserProfile>,
    /// Connections whose profile could not be read, with the reason.
    #[serde(default)]
    pub failures: Vec<ComposioIdentityFailure>,
}

/// One connection whose identity could not be read.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposioIdentityFailure {
    /// Connection that failed.
    pub connection_id: String,
    /// Toolkit it belongs to.
    pub toolkit: String,
    /// Why the read failed.
    pub message: String,
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
