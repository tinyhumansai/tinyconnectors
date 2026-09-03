//! The Notion provider.

use async_trait::async_trait;

use super::identity::pick;
use super::notion_catalog::CURATED;
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
    action: "NOTION_FETCH_DATA",
    item_pointers: &["/data/results", "/results", "/data/data/results"],
    id_paths: &["id", "page_id"],
    title_paths: &["title", "properties.title.title.0.plain_text"],
    content_paths: &["content", "markdown", "plain_text"],
    url_paths: &["url", "public_url"],
    version_paths: &["last_edited_time"],
    page_size_arg: "page_size",
    depth_window: None,
    cursor_arg: "start_cursor",
    clean_bodies: false,
};

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

    async fn fetch_user_profile(&self, context: &ProviderContext) -> Result<ProviderUserProfile> {
        let payload = context.run(PROFILE_ACTION, serde_json::json!({})).await?;
        Ok(ProviderUserProfile {
            toolkit: self.toolkit_slug().to_string(),
            connection_id: Some(context.connection_id.clone()),
            display_name: pick(&payload, &["name"]),
            email: pick(&payload, &["person.email", "email"]),
            avatar_url: pick(&payload, &["avatar_url"]),
            username: pick(&payload, &["id"]),
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
