//! The provider trait and its value types.

use super::context::ProviderContext;
use crate::Result;
use crate::scope::CuratedTool;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

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

    /// The actions worth offering an agent, if this provider curates them.
    ///
    /// `None` means uncurated: every action passes through, and scope gating
    /// falls back to [`crate::classify_unknown`].
    fn curated_tools(&self) -> Option<&'static [CuratedTool]> {
        None
    }

    /// Read the connected account's identity.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::Action`] when the underlying action fails.
    async fn fetch_user_profile(&self, context: &ProviderContext) -> Result<ProviderUserProfile>;
}
