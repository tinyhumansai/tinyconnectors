//! Trigger catalog, subscription, and delivery payloads.

use serde::{Deserialize, Serialize};

use super::super::serde_compat::{de_opt_string_or_object, de_string_or_object};

/// Per-repository descriptor for a GitHub-scoped available trigger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposioAvailableTriggerRepo {
    /// Owning user or organization login.
    pub owner: String,
    /// Repository name, without the owner.
    pub repo: String,
}

/// One trigger a user could enable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposioAvailableTrigger {
    /// Trigger slug, e.g. `"GMAIL_NEW_GMAIL_MESSAGE"`.
    pub slug: String,
    /// `"static"` for a toolkit-wide trigger, `"github_repo"` for one bound to
    /// a single repository.
    pub scope: String,
    /// Configuration the backend will apply unless the caller overrides it.
    #[serde(
        rename = "defaultConfig",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub default_config: Option<serde_json::Value>,
    /// Configuration keys the caller must supply for the trigger to enable.
    #[serde(
        rename = "requiredConfigKeys",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub required_config_keys: Option<Vec<String>>,
    /// Repository this row is scoped to, for `"github_repo"` triggers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<ComposioAvailableTriggerRepo>,
}

/// Response body of `GET /agent-integrations/composio/triggers/available`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposioAvailableTriggersResponse {
    /// Triggers the user could enable.
    #[serde(default)]
    pub triggers: Vec<ComposioAvailableTrigger>,
}

/// One enabled trigger subscription.
///
/// Every required string field decodes leniently — see the module docs for why.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposioActiveTrigger {
    /// Subscription id. This is what a caller deletes to disable it.
    #[serde(deserialize_with = "de_string_or_object")]
    pub id: String,
    /// Trigger slug, e.g. `"GMAIL_NEW_GMAIL_MESSAGE"`.
    #[serde(deserialize_with = "de_string_or_object")]
    pub slug: String,
    /// Toolkit slug the trigger belongs to.
    #[serde(deserialize_with = "de_string_or_object")]
    pub toolkit: String,
    /// Connection the subscription was made through.
    #[serde(rename = "connectionId", deserialize_with = "de_string_or_object")]
    pub connection_id: String,
    /// Configuration the subscription was enabled with.
    #[serde(
        rename = "triggerConfig",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub trigger_config: Option<serde_json::Value>,
    /// Upstream lifecycle state, when reported.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_opt_string_or_object"
    )]
    pub state: Option<String>,
}

/// Response body of `GET /agent-integrations/composio/triggers`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposioActiveTriggersResponse {
    /// Currently enabled subscriptions.
    #[serde(default)]
    pub triggers: Vec<ComposioActiveTrigger>,
}

/// Response body of `POST /agent-integrations/composio/triggers`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposioCreateTriggerResponse {
    /// Id of the subscription that was created.
    #[serde(rename = "triggerId")]
    pub trigger_id: String,
    /// Upstream status of the new subscription, when reported.
    #[serde(default)]
    pub status: Option<String>,
}

/// Response body of the enable path of `POST /agent-integrations/composio/triggers`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposioEnableTriggerResponse {
    /// Id of the subscription that was enabled.
    #[serde(rename = "triggerId")]
    pub trigger_id: String,
    /// Trigger slug that was enabled.
    pub slug: String,
    /// Connection the subscription was made through.
    #[serde(rename = "connectionId")]
    pub connection_id: String,
}

/// Response body of `DELETE /agent-integrations/composio/triggers/:id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposioDisableTriggerResponse {
    /// Whether the subscription was removed.
    #[serde(default)]
    pub deleted: bool,
}

/// One webhook delivery, as the backend fans it out to a user's sockets.
///
/// Every field defaults: a delivery that arrives without a recognizable body
/// should still be recorded as having arrived rather than dropped on a decode
/// error.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposioTriggerEvent {
    /// Toolkit slug, e.g. `"gmail"`.
    #[serde(default)]
    pub toolkit: String,
    /// Trigger slug, e.g. `"GMAIL_NEW_GMAIL_MESSAGE"`.
    #[serde(default)]
    pub trigger: String,
    /// Trigger-specific payload, in the provider's own shape.
    #[serde(default)]
    pub payload: serde_json::Value,
    /// Identifiers the backend attaches to the delivery.
    #[serde(default)]
    pub metadata: ComposioTriggerMetadata,
}

/// Identifiers the backend attaches to a webhook delivery.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposioTriggerMetadata {
    /// Backend event id.
    #[serde(default)]
    pub id: String,
    /// Backend event UUID.
    #[serde(default)]
    pub uuid: String,
}

/// One archived delivery.
///
/// This is the module's own record, written to a daily JSONL file — not a
/// backend envelope. It flattens [`ComposioTriggerEvent`] and stamps an arrival
/// time so the archive is answerable without a second lookup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposioTriggerHistoryEntry {
    /// Unix timestamp in milliseconds when the trigger reached the module.
    pub received_at_ms: u64,
    /// Toolkit slug, e.g. `"gmail"`.
    pub toolkit: String,
    /// Trigger slug, e.g. `"GMAIL_NEW_GMAIL_MESSAGE"`.
    pub trigger: String,
    /// Backend metadata id for this event.
    pub metadata_id: String,
    /// Backend metadata UUID for this event.
    pub metadata_uuid: String,
    /// Raw provider payload, as forwarded.
    pub payload: serde_json::Value,
}

/// A window onto the trigger archive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposioTriggerHistoryResult {
    /// Directory holding the daily JSONL archives.
    pub archive_dir: String,
    /// Path of the file the current day is being written to.
    pub current_day_file: String,
    /// Recent deliveries, newest first.
    pub entries: Vec<ComposioTriggerHistoryEntry>,
}
