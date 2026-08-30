//! The curated github catalog.

use crate::scope::{CuratedTool, ToolScope};

/// Actions worth offering an agent for github.
///
/// Composio publishes far more than this. The long tail is edge-case
/// administration an agent never plans for, and offering all of it makes the
/// model's tool list worse, not better.
pub const CURATED: &[CuratedTool] = &[
    CuratedTool {
        slug: "GITHUB_GET_THE_AUTHENTICATED_USER",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GITHUB_LIST_REPOSITORIES_FOR_THE_AUTHENTICATED_USER",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GITHUB_GET_A_REPOSITORY",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GITHUB_LIST_REPOSITORY_COLLABORATORS",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GITHUB_SEARCH_REPOSITORIES",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GITHUB_SEARCH_CODE",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GITHUB_SEARCH_ISSUES_AND_PULL_REQUESTS",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GITHUB_SEARCH_USERS",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GITHUB_LIST_REPOSITORY_ISSUES",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GITHUB_GET_AN_ISSUE",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GITHUB_LIST_ISSUE_COMMENTS",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GITHUB_LIST_PULL_REQUESTS",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GITHUB_GET_A_PULL_REQUEST",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GITHUB_LIST_BRANCHES",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GITHUB_GET_A_BRANCH",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GITHUB_LIST_COMMITS",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GITHUB_GET_A_COMMIT",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GITHUB_CREATE_A_REPOSITORY_FOR_THE_AUTHENTICATED_USER",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "GITHUB_CREATE_OR_UPDATE_FILE_CONTENTS",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "GITHUB_CREATE_A_COMMIT",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "GITHUB_CREATE_A_COMMIT_COMMENT",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "GITHUB_CREATE_AN_ISSUE",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "GITHUB_UPDATE_AN_ISSUE",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "GITHUB_CREATE_AN_ISSUE_COMMENT",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "GITHUB_ADD_LABELS_TO_AN_ISSUE",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "GITHUB_ADD_ASSIGNEES_TO_AN_ISSUE",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "GITHUB_CREATE_A_PULL_REQUEST",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "GITHUB_UPDATE_A_PULL_REQUEST",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "GITHUB_MERGE_A_PULL_REQUEST",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "GITHUB_CREATE_A_REVIEW_FOR_A_PULL_REQUEST",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "GITHUB_CREATE_A_REVIEW_COMMENT_FOR_A_PULL_REQUEST",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "GITHUB_CREATE_A_GIST",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "GITHUB_DELETE_A_REPOSITORY",
        scope: ToolScope::Admin,
    },
    CuratedTool {
        slug: "GITHUB_DELETE_A_REFERENCE",
        scope: ToolScope::Admin,
    },
    CuratedTool {
        slug: "GITHUB_DELETE_A_FILE",
        scope: ToolScope::Admin,
    },
    CuratedTool {
        slug: "GITHUB_ADD_A_REPOSITORY_COLLABORATOR",
        scope: ToolScope::Admin,
    },
    CuratedTool {
        slug: "GITHUB_CANCEL_A_WORKFLOW_RUN",
        scope: ToolScope::Admin,
    },
];
