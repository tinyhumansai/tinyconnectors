//! Unit tests for scope classification.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{CuratedTool, ToolScope, classify_unknown, find_curated, toolkit_from_slug};

#[test]
fn classifies_destructive_verbs_as_admin() {
    for slug in [
        "GMAIL_DELETE_EMAIL",
        "GMAIL_TRASH_EMAIL",
        "SLACK_REMOVE_USER",
        "GOOGLEDRIVE_SHARE_FILE",
        "GITHUB_REVOKE_TOKEN",
    ] {
        assert_eq!(classify_unknown(slug), ToolScope::Admin, "{slug}");
    }
}

#[test]
fn classifies_mutating_verbs_as_write() {
    for slug in [
        "GMAIL_SEND_EMAIL",
        "NOTION_CREATE_PAGE",
        "NOTION_UPDATE_PAGE",
        "SLACK_POST_MESSAGE",
        "GMAIL_DRAFT_REPLY",
    ] {
        assert_eq!(classify_unknown(slug), ToolScope::Write, "{slug}");
    }
}

#[test]
fn defaults_to_read_for_an_unrecognized_verb() {
    for slug in [
        "GMAIL_FETCH_EMAILS",
        "NOTION_SEARCH",
        "GMAIL_GET_PROFILE",
        "SOMETHING_INSCRUTABLE",
    ] {
        assert_eq!(classify_unknown(slug), ToolScope::Read, "{slug}");
    }
}

#[test]
fn checks_destructive_verbs_before_mutating_ones() {
    // `MODIFY_LABELS` is how Gmail deletes: it removes INBOX. Classified as
    // write, a read-and-write user could have their mail archived by an action
    // they never consented to.
    assert_eq!(classify_unknown("GMAIL_MODIFY_LABELS"), ToolScope::Admin);

    // Contains both `ADD` (write) and `REMOVE` (admin). Admin must win.
    assert_eq!(
        classify_unknown("GMAIL_ADD_AND_REMOVE_LABELS"),
        ToolScope::Admin
    );
}

#[test]
fn reads_the_verb_rather_than_the_noun() {
    // `GMAIL_LIST_DRAFTS` is a read: the noun is `DRAFTS`, the verb is `LIST`.
    // A plain substring scan calls it a write and hides it from a read-only
    // user — found by the curated catalogs disagreeing with the heuristic.
    assert_eq!(classify_unknown("GMAIL_LIST_DRAFTS"), ToolScope::Read);
    assert_eq!(classify_unknown("GMAIL_GET_DRAFT"), ToolScope::Read);
    assert_eq!(classify_unknown("NOTION_SEARCH_POSTS"), ToolScope::Read);
    assert_eq!(classify_unknown("GITHUB_LIST_CREATED_ISSUES"), ToolScope::Read);

    // The verb still decides when it is a write one.
    assert_eq!(classify_unknown("GMAIL_DRAFT_REPLY"), ToolScope::Write);
    assert_eq!(classify_unknown("GMAIL_CREATE_DRAFT"), ToolScope::Write);
}

#[test]
fn a_destructive_word_wins_over_a_reading_verb() {
    // `LIST` leads, but the action removes something.
    assert_eq!(classify_unknown("GMAIL_LIST_AND_DELETE"), ToolScope::Admin);
    assert_eq!(classify_unknown("GITHUB_GET_AND_REVOKE_TOKEN"), ToolScope::Admin);
}

#[test]
fn finds_the_verb_past_a_multi_segment_toolkit_name() {
    // Splitting on the first underscore would take `TEAMS` as the verb.
    assert_eq!(
        classify_unknown("MICROSOFT_TEAMS_LIST_DRAFTS"),
        ToolScope::Read
    );
    assert_eq!(
        classify_unknown("ONE_DRIVE_LIST_SHARED_ITEMS"),
        ToolScope::Admin,
        "SHARED still reads as destructive wherever it appears"
    );
}

#[test]
fn is_case_insensitive() {
    assert_eq!(classify_unknown("gmail_delete_email"), ToolScope::Admin);
    assert_eq!(classify_unknown("gmail_send_email"), ToolScope::Write);
}

#[test]
fn a_permission_limit_admits_everything_less_invasive() {
    // A user who consented to changes has necessarily consented to looking.
    assert!(ToolScope::Read.is_allowed_by(ToolScope::Write));
    assert!(ToolScope::Write.is_allowed_by(ToolScope::Write));
    assert!(!ToolScope::Admin.is_allowed_by(ToolScope::Write));

    assert!(ToolScope::Read.is_allowed_by(ToolScope::Read));
    assert!(!ToolScope::Write.is_allowed_by(ToolScope::Read));

    for scope in [ToolScope::Read, ToolScope::Write, ToolScope::Admin] {
        assert!(scope.is_allowed_by(ToolScope::Admin), "{scope:?}");
    }
}

#[test]
fn every_scope_has_a_stable_wire_name() {
    assert_eq!(ToolScope::Read.as_str(), "read");
    assert_eq!(ToolScope::Write.as_str(), "write");
    assert_eq!(ToolScope::Admin.as_str(), "admin");

    for scope in [ToolScope::Read, ToolScope::Write, ToolScope::Admin] {
        let value = serde_json::to_value(scope).expect("serializes");
        assert_eq!(value, serde_json::json!(scope.as_str()));
    }
}

#[test]
fn finds_a_curated_tool_regardless_of_casing() {
    const CATALOG: &[CuratedTool] = &[
        CuratedTool {
            slug: "GMAIL_SEND_EMAIL",
            scope: ToolScope::Write,
        },
        CuratedTool {
            slug: "GMAIL_FETCH_EMAILS",
            scope: ToolScope::Read,
        },
    ];

    let found = find_curated(CATALOG, "gmail_send_email").expect("found");
    assert_eq!(found.scope, ToolScope::Write);
    assert!(find_curated(CATALOG, "GMAIL_DELETE_EMAIL").is_none());
}

#[test]
fn derives_the_toolkit_from_an_action_slug() {
    assert_eq!(
        toolkit_from_slug("GMAIL_SEND_EMAIL").as_deref(),
        Some("gmail")
    );
    assert_eq!(
        toolkit_from_slug("  NOTION_SEARCH  ").as_deref(),
        Some("notion")
    );
}

#[test]
fn keeps_a_toolkit_whose_own_name_contains_an_underscore() {
    // Splitting on the first underscore would give "microsoft", which matches
    // no connected toolkit — silently dropping every action of that toolkit.
    for (slug, toolkit) in [
        ("MICROSOFT_TEAMS_SEND_MESSAGE", "microsoft_teams"),
        ("ONE_DRIVE_LIST_FILES", "one_drive"),
        ("ZOHO_MAIL_FETCH", "zoho_mail"),
    ] {
        assert_eq!(toolkit_from_slug(slug).as_deref(), Some(toolkit), "{slug}");
    }
}

#[test]
fn has_no_toolkit_for_an_empty_slug() {
    assert!(toolkit_from_slug("").is_none());
    assert!(toolkit_from_slug("   ").is_none());
    assert!(toolkit_from_slug("_LEADING").is_none());
}
