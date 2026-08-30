//! The Gmail provider.

use async_trait::async_trait;

use super::gmail_catalog::CURATED;
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
    action: "GMAIL_FETCH_EMAILS",
    item_pointers: &[
        "/data/messages",
        "/data/data/messages",
        "/messages",
        "/data/items",
    ],
    id_paths: &["id", "messageId", "message_id"],
    title_paths: &["subject", "payload.headers.0.value"],
    content_paths: &["messageText", "snippet", "body", "preview"],
    url_paths: &["messageUrl", "webLink"],
    version_paths: &["historyId", "internalDate"],
    page_size_arg: "max_results",
    cursor_arg: "page_token",
    clean_bodies: true,
};

/// The action that reads the connected account's identity.
const PROFILE_ACTION: &str = "GMAIL_GET_PROFILE";

/// Gmail as a connector toolkit.
#[derive(Debug, Default, Clone, Copy)]
pub struct GmailProvider;

#[async_trait]
impl ConnectorProvider for GmailProvider {
    fn toolkit_slug(&self) -> &'static str {
        "gmail"
    }

    fn description(&self) -> &'static str {
        "Read and send email, and ingest recent mail as memory."
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
            email: pick(&payload, &["emailAddress", "email"]),
            display_name: pick(&payload, &["name", "displayName"]),
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
