//! The Slack provider: its identity read and its curated action catalog.

use async_trait::async_trait;

use super::slack_catalog::CURATED;
use super::identity::pick;
use crate::Result;
use crate::provider::{ConnectorProvider, ProviderContext, ProviderUserProfile};
use crate::scope::CuratedTool;

/// Reads the workspace the connection belongs to.
const PROFILE_ACTION: &str = "SLACK_FETCH_TEAM_INFO";

/// Slack as a connector toolkit.
#[derive(Debug, Default, Clone, Copy)]
pub struct SlackProvider;

#[async_trait]
impl ConnectorProvider for SlackProvider {
    fn toolkit_slug(&self) -> &'static str {
        "slack"
    }

    fn description(&self) -> &'static str {
        "Read and send Slack messages."
    }

    fn curated_tools(&self) -> Option<&'static [CuratedTool]> {
        Some(CURATED)
    }

    async fn fetch_user_profile(&self, context: &ProviderContext) -> Result<ProviderUserProfile> {
        let payload = context.run(PROFILE_ACTION, serde_json::json!({})).await?;
        Ok(ProviderUserProfile {
            toolkit: self.toolkit_slug().to_string(),
            connection_id: Some(context.connection_id.clone()),
            // The workspace is what a Slack connection is *of*; a UI labelling
            // an account has nothing better to show.
            display_name: pick(&payload, &["team.name", "name", "data.team.name"]),
            username: pick(&payload, &["team.domain", "domain", "data.team.domain"]),
            avatar_url: pick(&payload, &["team.icon.image_132", "icon.image_132"]),
            extras: payload,
            ..ProviderUserProfile::default()
        })
    }

}
