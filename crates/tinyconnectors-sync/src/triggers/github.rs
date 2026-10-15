//! Curated triggers — GitHub.
//!
//! Everything but the assigned-issue trigger watches one repository, so a host
//! must ask the user which (`owner`, `repo`, and for reviews `pull_number`).

use super::types::{CuratedTrigger, TriggerDelivery, TriggerEvent, TriggerTier};

// ── github ──────────────────────────────────────────────────────────
/// The curated triggers of the `github` toolkit.
pub(super) const GITHUB_TRIGGERS: &[CuratedTrigger] = &[
    CuratedTrigger {
        slug: "GITHUB_ISSUE_ASSIGNED_TO_ME_TRIGGER",
        toolkit: "github",
        event: TriggerEvent::IssueAssignedToMe,
        delivery: TriggerDelivery::Poll,
        tier: TriggerTier::Extended,
        required_config: &[],
        optional_config: &["interval", "labels", "state"],
        use_case: "Tell the user when an issue lands on their plate, and offer to add it to \
                   their tasks.",
    },
    CuratedTrigger {
        slug: "GITHUB_PULL_REQUEST_EVENT",
        toolkit: "github",
        event: TriggerEvent::PullRequestActivity,
        delivery: TriggerDelivery::Webhook,
        tier: TriggerTier::PerResource,
        required_config: &["owner", "repo"],
        optional_config: &[],
        use_case: "Follow pull requests on a repository the user owns or maintains.",
    },
    CuratedTrigger {
        slug: "GITHUB_REPOSITORY_NOTIFICATION_RECEIVED_TRIGGER",
        toolkit: "github",
        event: TriggerEvent::RepositoryNotification,
        delivery: TriggerDelivery::Poll,
        tier: TriggerTier::PerResource,
        required_config: &["owner", "repo"],
        optional_config: &["all", "interval", "participating"],
        use_case: "Relay the user's GitHub notifications for a repository they follow closely.",
    },
    CuratedTrigger {
        slug: "GITHUB_PULL_REQUEST_REVIEW_SUBMITTED_TRIGGER",
        toolkit: "github",
        event: TriggerEvent::PullRequestReviewSubmitted,
        delivery: TriggerDelivery::Poll,
        tier: TriggerTier::PerResource,
        required_config: &["owner", "repo", "pull_number"],
        optional_config: &["interval"],
        use_case: "Tell the user when a review lands on a pull request they are waiting on.",
    },
];
