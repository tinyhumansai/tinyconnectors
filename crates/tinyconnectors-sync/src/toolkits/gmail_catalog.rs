//! The curated gmail catalog.

use crate::scope::{CuratedTool, ToolScope};

/// Actions worth offering an agent for gmail.
///
/// Composio publishes far more than this. The long tail is edge-case
/// administration an agent never plans for, and offering all of it makes the
/// model's tool list worse, not better.
pub const CURATED: &[CuratedTool] = &[
    CuratedTool {
        slug: "GMAIL_FETCH_EMAILS",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GMAIL_LIST_MESSAGES",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GMAIL_FETCH_MESSAGE_BY_MESSAGE_ID",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GMAIL_FETCH_MESSAGE_BY_THREAD_ID",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GMAIL_LIST_THREADS",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GMAIL_GET_ATTACHMENT",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GMAIL_GET_PROFILE",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GMAIL_GET_CONTACTS",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GMAIL_GET_PEOPLE",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GMAIL_SEARCH_PEOPLE",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GMAIL_LIST_DRAFTS",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GMAIL_GET_DRAFT",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GMAIL_LIST_LABELS",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GMAIL_GET_LABEL",
        scope: ToolScope::Read,
    },
    CuratedTool {
        slug: "GMAIL_SEND_EMAIL",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "GMAIL_REPLY_TO_THREAD",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "GMAIL_FORWARD_MESSAGE",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "GMAIL_CREATE_EMAIL_DRAFT",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "GMAIL_UPDATE_DRAFT",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "GMAIL_SEND_DRAFT",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "GMAIL_ADD_LABEL_TO_EMAIL",
        scope: ToolScope::Write,
    },
    CuratedTool {
        slug: "GMAIL_DELETE_MESSAGE",
        scope: ToolScope::Admin,
    },
    CuratedTool {
        slug: "GMAIL_BATCH_DELETE_MESSAGES",
        scope: ToolScope::Admin,
    },
    CuratedTool {
        slug: "GMAIL_MOVE_TO_TRASH",
        scope: ToolScope::Admin,
    },
    CuratedTool {
        slug: "GMAIL_DELETE_THREAD",
        scope: ToolScope::Admin,
    },
    CuratedTool {
        slug: "GMAIL_MOVE_THREAD_TO_TRASH",
        scope: ToolScope::Admin,
    },
    CuratedTool {
        slug: "GMAIL_UNTRASH_THREAD",
        scope: ToolScope::Admin,
    },
    CuratedTool {
        slug: "GMAIL_DELETE_DRAFT",
        scope: ToolScope::Admin,
    },
    CuratedTool {
        slug: "GMAIL_DELETE_LABEL",
        scope: ToolScope::Admin,
    },
];
