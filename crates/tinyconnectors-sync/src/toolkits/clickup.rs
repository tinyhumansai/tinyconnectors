//! The `ClickUp` provider.
//!
//! `ClickUp` reads tasks one workspace at a time: its task search needs the
//! workspace's `team_id`, and an account can belong to several. A walk lists
//! the account's workspaces on every page read and carries its position as
//! `"<workspace>|<page>"`. Listing each time costs a second request per page,
//! which the page reports so the day's budget counts it; in exchange, a
//! workspace the account has since left sends the walk back to the start
//! instead of failing every run that resumes into it.

use async_trait::async_trait;
use serde_json::{Value, json};

use super::clickup_catalog::CURATED;
use super::identity::pick;
use crate::Result;
use crate::pipeline::{PageSpec, Paging, ProviderPage, fetch_page_with, first_array, pick_str};
use crate::provider::{ConnectorProvider, ProviderContext, ProviderUserProfile};
use crate::scope::CuratedTool;

/// How one page of a workspace's tasks is read.
///
/// The paths are alternatives, tried in order: Composio wraps provider payloads
/// inconsistently, and the same field arrives under different names from
/// different endpoints of the same API.
///
/// The workspace, and the arguments that are not strings, are added to each
/// read by [`ClickupProvider::fetch_page`].
const PAGE: PageSpec = PageSpec {
    action: "CLICKUP_GET_FILTERED_TEAM_TASKS",
    item_pointers: &["/data/tasks", "/tasks", "/data/data/tasks"],
    id_paths: &["id", "custom_id"],
    title_paths: &["name"],
    content_paths: &["description", "text_content"],
    url_paths: &["url"],
    version_paths: &["date_updated"],
    // Most recently updated first, with `reverse`, so whatever changed since
    // the last run is on the first page.
    fixed_arguments: &[("order_by", "updated")],
    // `ClickUp` serves up to a hundred tasks a page whatever it is asked, so a
    // page at least this full reads on.
    page_size_arg: "page_size",
    depth_window: None,
    cursor_arg: "page",
    // Pages count from zero, and the payload flags the last one.
    paging: Paging::Numbered {
        first: 0,
        reachable: None,
        last_page: &["/data/last_page", "/last_page", "/data/data/last_page"],
    },
    clean_bodies: false,
};

/// The action that reads the connected account's identity.
const PROFILE_ACTION: &str = "CLICKUP_GET_AUTHORIZED_USER";

/// The action that lists the workspaces the connected account belongs to.
const WORKSPACES_ACTION: &str = "CLICKUP_GET_AUTHORIZED_TEAMS_WORKSPACES";

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
        let workspaces = workspace_ids(&context.run(WORKSPACES_ACTION, json!({})).await?);
        // Resume where the cursor says, unless it names a workspace the account
        // no longer lists: then start again from the first.
        let resumed = cursor
            .and_then(decode_cursor)
            .filter(|(workspace, _)| workspaces.contains(workspace));
        let Some((workspace, page)) =
            resumed.or_else(|| workspaces.first().map(|first| (first.clone(), 0)))
        else {
            return Ok(ProviderPage::default());
        };

        let this_read = [
            ("team_id", Value::String(workspace.clone())),
            ("reverse", Value::Bool(true)),
            ("subtasks", Value::Bool(true)),
        ];
        let mut read = fetch_page_with(context, Some(&page.to_string()), &PAGE, &this_read).await?;
        read.next_cursor = match read.next_cursor.take() {
            Some(next_page) => Some(format!("{workspace}|{next_page}")),
            None => next_workspace(&workspaces, &workspace).map(|next| format!("{next}|0")),
        };
        // The workspace list and the task page: the day's budget counts both.
        read.requests_used = 2;
        Ok(read)
    }
}

/// The workspace ids a workspaces payload lists, in its order, each once.
///
/// Once each: the walk moves to the workspace listed after the one it
/// finished, found by position, so an id listed twice would send the walk back
/// into itself and it would never reach the workspaces after it.
fn workspace_ids(payload: &Value) -> Vec<String> {
    let mut ids = Vec::new();
    for id in first_array(
        payload,
        &["/teams", "/data/teams", "/workspaces", "/data/workspaces"],
    )
    .iter()
    .filter_map(|workspace| pick_str(workspace, &["id", "team_id", "workspace_id"]))
    {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

/// The workspace and page a cursor names, or `None` when it names neither.
fn decode_cursor(cursor: &str) -> Option<(String, u32)> {
    let (workspace, page) = cursor.rsplit_once('|')?;
    let page = page.parse().ok()?;
    (!workspace.is_empty()).then(|| (workspace.to_string(), page))
}

/// The workspace listed after `current`, if there is one.
fn next_workspace<'a>(workspaces: &'a [String], current: &str) -> Option<&'a str> {
    let index = workspaces
        .iter()
        .position(|workspace| workspace == current)?;
    workspaces.get(index + 1).map(String::as_str)
}

#[cfg(test)]
#[path = "clickup_tests.rs"]
mod test;
