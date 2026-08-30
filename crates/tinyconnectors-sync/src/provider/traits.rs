//! The provider trait and its value types.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tinyconnectors_bus::ConnectorRecordBatch;

use super::context::ProviderContext;
use crate::Result;
use crate::scope::CuratedTool;

/// Why a sync run was started.
///
/// Recorded on the outcome so an operator reading a log can tell a scheduled
/// run from one a user asked for — the two have very different expectations
/// about how long they may take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncReason {
    /// The first run after a connection was made.
    InitialConnect,
    /// The periodic scheduler.
    Scheduled,
    /// A user asked for it.
    Manual,
    /// A webhook said something changed.
    Trigger,
}

impl SyncReason {
    /// The stable wire name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InitialConnect => "initial_connect",
            Self::Scheduled => "scheduled",
            Self::Manual => "manual",
            Self::Trigger => "trigger",
        }
    }
}

/// The connected account's identity, as far as a toolkit reports it.
///
/// Every field is optional because the toolkits disagree about which they have:
/// Gmail knows an email, Slack knows a workspace and a display name, GitHub
/// knows a login. A UI picking a label falls back through them in that order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderUserProfile {
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
    /// Anything toolkit-specific.
    ///
    /// Here so a new toolkit with an interesting field does not require
    /// widening this shape — and every consumer of it — to carry something one
    /// provider reports.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub extras: serde_json::Value,
}

/// What a connector knows about one toolkit.
#[async_trait]
pub trait ConnectorProvider: Send + Sync + std::fmt::Debug {
    /// Toolkit slug, e.g. `"gmail"`.
    ///
    /// Must match the slug Composio uses: the registry keys on it, and a
    /// mismatch means the provider is simply never found.
    fn toolkit_slug(&self) -> &'static str;

    /// A one-line description of what connecting this toolkit gets the user.
    fn description(&self) -> &'static str;

    /// How often to re-sync, in seconds.
    ///
    /// `None` opts out of the periodic scheduler entirely — right for a
    /// write-only toolkit, where there is nothing to read back.
    fn sync_interval_secs(&self) -> Option<u64> {
        Some(15 * 60)
    }

    /// The actions worth offering an agent, if this provider curates them.
    ///
    /// `None` means uncurated: every action passes through, and scope gating
    /// falls back to [`crate::classify_unknown`].
    fn curated_tools(&self) -> Option<&'static [CuratedTool]> {
        None
    }

    /// Whether this provider can produce records at all.
    ///
    /// False for toolkits that are useful to act through but have nothing to
    /// ingest. Distinguishing them is what lets a UI say "connected, and the
    /// agent can use it" rather than implying a sync that will never run.
    fn can_sync(&self) -> bool {
        true
    }

    /// Read the connected account's identity.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::Action`] when the underlying action fails.
    async fn fetch_user_profile(&self, context: &ProviderContext)
    -> Result<ProviderUserProfile>;

    /// Read one batch of records.
    ///
    /// Returns what it read plus whether more remains — the caller drives
    /// resumption rather than the provider looping internally, so a run can be
    /// stopped between batches.
    ///
    /// The default produces nothing and reports completion, which is right for
    /// a provider whose [`Self::can_sync`] is false.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::Action`] when a provider call fails, or
    /// [`crate::Error::Store`] when sync state cannot be read or written.
    async fn fetch_records(&self, context: &ProviderContext) -> Result<ConnectorRecordBatch> {
        Ok(ConnectorRecordBatch {
            source_id: context.source_id.clone(),
            toolkit: context.toolkit.clone(),
            connection_id: Some(context.connection_id.clone()),
            records: Vec::new(),
            cursor: None,
            complete: true,
        })
    }
}
