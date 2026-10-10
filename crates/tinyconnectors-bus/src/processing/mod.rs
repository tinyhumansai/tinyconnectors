//! Argument shaping and structured provider failures exchanged with the module.
use crate::ComposioExecuteResponse;
use serde::{Deserialize, Serialize};

/// Host-selected shaping context; algorithms execute inside the module.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PrepareArgumentsRequest {
    /// Action identifier.
    pub tool: String,
    /// Caller arguments.
    pub arguments: Option<serde_json::Value>,
    /// Optional host-resolved IANA time zone.
    pub timezone: Option<String>,
    /// Optional host-selected recency boundary as an RFC3339 instant.
    pub since: Option<String>,
}
/// Structured failure for product presentation, never telemetry payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderError {
    /// Stable classification identifier used by the existing provider formatter.
    pub class: String,
    /// Existing user-facing message.
    pub message: String,
}
/// Prepared arguments or a classified validation failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedArguments {
    /// Arguments when valid.
    pub arguments: Option<serde_json::Value>,
    /// Failure when preparation was refused.
    pub error: Option<ProviderError>,
}
/// Provider error classification input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifyErrorRequest {
    /// Action identifier.
    pub tool: String,
    /// Provider error detail; not suitable for telemetry.
    pub message: String,
}
/// Response filtering input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterResponseRequest {
    /// Action identifier.
    pub tool: String,
    /// Provider response to filter.
    pub response: ComposioExecuteResponse,
    /// Host-selected RFC3339 recency boundary.
    pub since: String,
}

/// Opaque module-owned trigger archive lease.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ArchiveHandle(pub String);
/// Open an archive beneath a host-validated state directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenArchiveRequest {
    /// Validated state directory; the module preserves the existing triggers subdirectory.
    pub state_dir: String,
}
/// Delivery to append to an explicitly owned archive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordTriggerRequest {
    /// Archive lease.
    pub handle: ArchiveHandle,
    /// Delivered event.
    pub event: crate::ComposioTriggerEvent,
}
/// Read recent deliveries from an owned archive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadArchiveRequest {
    /// Archive lease.
    pub handle: ArchiveHandle,
    /// Maximum recent deliveries, using the existing default when absent.
    pub limit: Option<usize>,
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
