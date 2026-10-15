//! Curated triggers — docs and tasks: Linear and Notion.
//!
//! Linear's triggers each watch one team (`team_id`). Notion's page-created
//! trigger is only useful under a parent page, which Composio leaves optional.

use super::types::{CuratedTrigger, TriggerDelivery, TriggerEvent, TriggerTier};

// ── linear ──────────────────────────────────────────────────────────
/// The curated triggers of the `linear` toolkit.
pub(super) const LINEAR_TRIGGERS: &[CuratedTrigger] = &[
    CuratedTrigger {
        slug: "LINEAR_ISSUE_CREATED_TRIGGER",
        toolkit: "linear",
        event: TriggerEvent::IssueCreated,
        delivery: TriggerDelivery::Webhook,
        tier: TriggerTier::PerResource,
        required_config: &["team_id"],
        optional_config: &[],
        use_case: "Tell the user about new issues in a team they work with.",
    },
    CuratedTrigger {
        slug: "LINEAR_ISSUE_UPDATED_TRIGGER",
        toolkit: "linear",
        event: TriggerEvent::IssueUpdated,
        delivery: TriggerDelivery::Webhook,
        tier: TriggerTier::PerResource,
        required_config: &["team_id"],
        optional_config: &[],
        use_case: "Track status, priority and assignee changes on a team's issues.",
    },
    CuratedTrigger {
        slug: "LINEAR_COMMENT_EVENT_TRIGGER",
        toolkit: "linear",
        event: TriggerEvent::IssueComment,
        delivery: TriggerDelivery::Webhook,
        tier: TriggerTier::PerResource,
        required_config: &["team_id"],
        optional_config: &[],
        use_case: "Surface comments on a team's issues that may need the user's answer.",
    },
];

// ── notion ──────────────────────────────────────────────────────────
/// The curated triggers of the `notion` toolkit.
pub(super) const NOTION_TRIGGERS: &[CuratedTrigger] = &[
    CuratedTrigger {
        slug: "NOTION_COMMENT_CREATED",
        toolkit: "notion",
        event: TriggerEvent::PageComment,
        delivery: TriggerDelivery::Webhook,
        tier: TriggerTier::Extended,
        required_config: &[],
        optional_config: &["page_id"],
        use_case: "Surface comments on the user's pages that may need an answer.",
    },
    CuratedTrigger {
        slug: "NOTION_PAGE_CREATED",
        toolkit: "notion",
        event: TriggerEvent::PageCreated,
        delivery: TriggerDelivery::Webhook,
        tier: TriggerTier::PerResource,
        required_config: &[],
        optional_config: &["data_source_id", "parent_page_id"],
        use_case: "Notice new pages under a parent the user watches, such as meeting notes or \
                   a team wiki.",
    },
];
