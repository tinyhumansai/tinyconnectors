//! The Linear provider.

use async_trait::async_trait;

use super::identity::pick;
use super::linear_catalog::CURATED;
use crate::Result;
use crate::pipeline::{PageSpec, Paging, ProviderPage, fetch_page};
use crate::provider::{ConnectorProvider, ProviderContext, ProviderUserProfile};
use crate::scope::CuratedTool;

/// How one page of this toolkit is read.
///
/// The paths are alternatives, tried in order: Composio wraps provider payloads
/// inconsistently, and the same field arrives under different names from
/// different endpoints of the same API.
const PAGE: PageSpec = PageSpec {
    action: "LINEAR_LIST_LINEAR_ISSUES",
    item_pointers: &[
        "/data/issues",
        "/issues",
        "/data/data/issues",
        "/data/issues/nodes",
        "/data/nodes",
    ],
    id_paths: &["id", "identifier"],
    title_paths: &["title"],
    content_paths: &["description", "descriptionData"],
    url_paths: &["url"],
    sender_paths: &[],
    version_paths: &["updatedAt"],
    fixed_arguments: &[],
    page_size_arg: "first",
    depth_window: None,
    cursor_arg: "after",
    // Issues arrive as a GraphQL connection, whose `pageInfo` sits beside its
    // `nodes`: `endCursor` names the next page only while `hasNextPage` is true.
    paging: Paging::PageInfo {
        page_info: &[
            "/data/pageInfo",
            "/pageInfo",
            "/data/data/pageInfo",
            "/data/issues/pageInfo",
        ],
    },
    clean_bodies: false,
};

/// The action that reads the connected account's identity.
const PROFILE_ACTION: &str = "LINEAR_LIST_LINEAR_USERS";

/// Linear as a connector toolkit.
#[derive(Debug, Default, Clone, Copy)]
pub struct LinearProvider;

#[async_trait]
impl ConnectorProvider for LinearProvider {
    fn toolkit_slug(&self) -> &'static str {
        "linear"
    }

    fn description(&self) -> &'static str {
        "Read Linear issues and projects, and ingest them as memory."
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
            display_name: pick(&payload, &["name", "displayName"]),
            email: pick(&payload, &["email"]),
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
