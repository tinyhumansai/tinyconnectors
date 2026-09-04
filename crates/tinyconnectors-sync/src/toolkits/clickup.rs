//! The `ClickUp` provider.

use async_trait::async_trait;

use super::clickup_catalog::CURATED;
use super::identity::pick;
use crate::Result;
use crate::pipeline::{PageSpec, ProviderPage, fetch_page};
use crate::provider::{ConnectorProvider, ProviderContext, ProviderUserProfile};
use crate::scope::CuratedTool;

/// How one page of this toolkit is read.
///
/// The paths are alternatives, tried in order: Composio wraps provider payloads
/// inconsistently, and the same field arrives under different names from
/// different endpoints of the same API.
const PAGE: PageSpec = PageSpec {
    action: "CLICKUP_GET_FILTERED_TEAM_TASKS",
    item_pointers: &["/data/tasks", "/tasks", "/data/data/tasks"],
    id_paths: &["id", "custom_id"],
    title_paths: &["name"],
    content_paths: &["description", "text_content"],
    url_paths: &["url"],
    version_paths: &["date_updated"],
    page_size_arg: "page",
    depth_window: None,
    cursor_arg: "page",
    clean_bodies: false,
};

/// The action that reads the connected account's identity.
const PROFILE_ACTION: &str = "CLICKUP_GET_AUTHORIZED_USER";

/// `ClickUp` as a connector toolkit.
#[derive(Debug, Default, Clone, Copy)]
pub struct ClickupProvider;

#[async_trait]
impl ConnectorProvider for ClickupProvider {
    fn toolkit_slug(&self) -> &'static str {
        "clickup"
    }

    fn description(&self) -> &'static str {
        "Read `ClickUp` tasks and spaces, and ingest them as memory."
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
            username: pick(&payload, &["username"]),
            display_name: pick(&payload, &["username"]),
            email: pick(&payload, &["email"]),
            avatar_url: pick(&payload, &["profilePicture"]),
            // The whole payload, so a caller wanting a field this shape does
            // not name can still reach it.
            extras: payload,
            ..ProviderUserProfile::default()
        })
    }

    async fn fetch_page(
        &self,
        context: &ProviderContext,
        cursor: Option<&str>,
    ) -> Result<ProviderPage> {
        fetch_page(context, cursor, &PAGE).await
    }
}
