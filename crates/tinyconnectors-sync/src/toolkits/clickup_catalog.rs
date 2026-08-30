//! The curated clickup catalog.

use crate::scope::{CuratedTool, ToolScope};

/// Actions worth offering an agent for clickup.
///
/// Composio publishes far more than this. The long tail is edge-case
/// administration an agent never plans for, and offering all of it makes the
/// model's tool list worse, not better.
pub(super) const CURATED: &[CuratedTool] = &[
    CuratedTool {
        slug: "CLICKUP_GET_AUTHORIZED_USER",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "CLICKUP_GET_AUTHORIZED_TEAMS_WORKSPACES",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "CLICKUP_GET_SPACES",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "CLICKUP_GET_FOLDERS",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "CLICKUP_GET_LISTS",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "CLICKUP_GET_FOLDERLESS_LISTS",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "CLICKUP_GET_FILTERED_TEAM_TASKS",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "CLICKUP_GET_TASKS",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "CLICKUP_GET_TASK",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "CLICKUP_GET_TASK_COMMENTS",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "CLICKUP_GET_LIST_COMMENTS",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "CLICKUP_SEARCH_DOCS",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "CLICKUP_GET_DOC_PAGES",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "CLICKUP_GET_VIEW_TASKS",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "CLICKUP_GET_TIME_ENTRIES_WITHIN_A_DATE_RANGE",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "CLICKUP_GET_WORKSPACE_MEMBERS",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "CLICKUP_GET_TASK_MEMBERS",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "CLICKUP_CREATE_TASK",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "CLICKUP_UPDATE_TASK",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "CLICKUP_CREATE_TASK_COMMENT",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "CLICKUP_UPDATE_COMMENT",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "CLICKUP_CREATE_LIST",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "CLICKUP_UPDATE_LIST",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "CLICKUP_DELETE_TASK",
        scope: ToolScope::Admin,
    },
    CuratedTool {
        slug: "CLICKUP_DELETE_COMMENT",
        scope: ToolScope::Admin,
    },
    CuratedTool {
        slug: "CLICKUP_DELETE_LIST",
        scope: ToolScope::Admin,
    },
];
