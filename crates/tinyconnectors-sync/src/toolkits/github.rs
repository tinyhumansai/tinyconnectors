//! The GitHub provider.

use async_trait::async_trait;

use super::github_catalog::CURATED;
use super::identity::pick;
use crate::Result;
use crate::provider::{ConnectorProvider, ProviderContext, ProviderUserProfile};
use crate::scope::CuratedTool;

/// The action that reads the connected account's identity.
const PROFILE_ACTION: &str = "GITHUB_GET_THE_AUTHENTICATED_USER";

/// GitHub as a connector toolkit.
#[derive(Debug, Default, Clone, Copy)]
pub struct GithubProvider;

#[async_trait]
impl ConnectorProvider for GithubProvider {
    fn toolkit_slug(&self) -> &'static str {
        "github"
    }

    fn description(&self) -> &'static str {
        "Read issues, pull requests, and repositories, and ingest them as memory."
    }

    fn curated_tools(&self) -> Option<&'static [CuratedTool]> {
        Some(CURATED)
    }

    fn sync_interval_secs(&self) -> Option<u64> {
        Some(900)
    }

    async fn fetch_user_profile(&self, context: &ProviderContext) -> Result<ProviderUserProfile> {
        let payload = context.run(PROFILE_ACTION, serde_json::json!({})).await?;
        Ok(ProviderUserProfile {
            toolkit: self.toolkit_slug().to_string(),
            connection_id: Some(context.connection_id.clone()),
            username: pick(&payload, &["login"]),
            display_name: pick(&payload, &["name"]),
            email: pick(&payload, &["email"]),
            avatar_url: pick(&payload, &["avatar_url"]),
            profile_url: pick(&payload, &["html_url"]),
            // The whole payload, so a caller wanting a field this shape does
            // not name can still reach it.
            extras: payload,
        })
    }
}
