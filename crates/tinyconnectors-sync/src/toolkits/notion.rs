//! The Notion provider.

use async_trait::async_trait;

use super::identity::pick;
use super::notion_catalog::CURATED;
use crate::provider::{ConnectorProvider, ProviderContext, ProviderUserProfile};
use crate::scope::CuratedTool;
use crate::Result;

/// The action that reads the connected account's identity.
const PROFILE_ACTION: &str = "NOTION_GET_ABOUT_ME";

/// Notion as a connector toolkit.
#[derive(Debug, Default, Clone, Copy)]
pub struct NotionProvider;

#[async_trait]
impl ConnectorProvider for NotionProvider {
    fn toolkit_slug(&self) -> &'static str {
        "notion"
    }

    fn description(&self) -> &'static str {
        "Read and search a Notion workspace, and ingest its pages as memory."
    }

    fn curated_tools(&self) -> Option<&'static [CuratedTool]> {
        Some(CURATED)
    }

    fn sync_interval_secs(&self) -> Option<u64> {
        Some(1800)
    }

    async fn fetch_user_profile(
        &self,
        context: &ProviderContext,
    ) -> Result<ProviderUserProfile> {
        let payload = context.run(PROFILE_ACTION, serde_json::json!({})).await?;
        Ok(ProviderUserProfile {
            toolkit: self.toolkit_slug().to_string(),
            connection_id: Some(context.connection_id.clone()),
            display_name: pick(&payload, &["name"]),
            email: pick(&payload, &["person.email", "email"]),
            avatar_url: pick(&payload, &["avatar_url"]),
            username: pick(&payload, &["id"]),
            // The whole payload, so a caller wanting a field this
            // shape does not name can still reach it.
            extras: payload,
        })
    }
}
