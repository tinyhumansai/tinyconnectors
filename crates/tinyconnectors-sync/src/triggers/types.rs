//! The curated trigger row and the enums that classify it.

use serde::{Deserialize, Serialize};

/// What happened, independent of the toolkit that reports it.
///
/// The key a host builds a feature on: "brief me before meetings" subscribes
/// to [`TriggerEvent::MeetingStartingSoon`] on whichever calendar the user
/// connected. Serialized in `snake_case`, the same string [`Self::as_str`]
/// returns; non-exhaustive because the catalog grows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TriggerEvent {
    /// Mail arrived in the user's inbox.
    NewEmail,
    /// The user sent mail.
    EmailSent,
    /// A calendar event is about to start.
    MeetingStartingSoon,
    /// An event was added to the user's calendar.
    CalendarEventCreated,
    /// An event on the user's calendar was canceled or deleted.
    CalendarEventCanceled,
    /// An attendee accepted, declined or tentatively accepted an event.
    AttendeeResponseChanged,
    /// Someone sent the user a direct message.
    DirectMessage,
    /// A message was posted in a channel the user can see.
    ChannelMessage,
    /// Someone reacted to a message.
    ReactionAdded,
    /// A file was shared with the user.
    FileShared,
    /// Someone commented on a document.
    DocumentComment,
    /// A page was created.
    PageCreated,
    /// Someone commented on a page.
    PageComment,
    /// An issue was assigned to the user.
    IssueAssignedToMe,
    /// An issue was created.
    IssueCreated,
    /// An issue was updated.
    IssueUpdated,
    /// Someone commented on an issue.
    IssueComment,
    /// A pull request was opened, updated, merged or closed.
    PullRequestActivity,
    /// A review was submitted on a pull request.
    PullRequestReviewSubmitted,
    /// A notification arrived for a repository.
    RepositoryNotification,
}

impl TriggerEvent {
    /// The stable wire name, the same string serde writes.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NewEmail => "new_email",
            Self::EmailSent => "email_sent",
            Self::MeetingStartingSoon => "meeting_starting_soon",
            Self::CalendarEventCreated => "calendar_event_created",
            Self::CalendarEventCanceled => "calendar_event_canceled",
            Self::AttendeeResponseChanged => "attendee_response_changed",
            Self::DirectMessage => "direct_message",
            Self::ChannelMessage => "channel_message",
            Self::ReactionAdded => "reaction_added",
            Self::FileShared => "file_shared",
            Self::DocumentComment => "document_comment",
            Self::PageCreated => "page_created",
            Self::PageComment => "page_comment",
            Self::IssueAssignedToMe => "issue_assigned_to_me",
            Self::IssueCreated => "issue_created",
            Self::IssueUpdated => "issue_updated",
            Self::IssueComment => "issue_comment",
            Self::PullRequestActivity => "pull_request_activity",
            Self::PullRequestReviewSubmitted => "pull_request_review_submitted",
            Self::RepositoryNotification => "repository_notification",
        }
    }
}

/// How the backend learns that the trigger fired.
///
/// A poll trigger takes an `interval` in minutes, so its latency is up to one
/// interval; a webhook trigger arrives when the upstream service pushes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerDelivery {
    /// The upstream service pushes the event.
    Webhook,
    /// The backend polls the upstream service on an interval.
    Poll,
}

/// How early a trigger is worth building on.
///
/// [`Self::Core`] and [`Self::Extended`] work account-wide with no
/// configuration. [`Self::PerResource`] watches one repository, pull request,
/// team or page, so a host has to ask the user which.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerTier {
    /// Build these first.
    Core,
    /// Worth building once the core set works.
    Extended,
    /// Needs a specific resource id in its configuration.
    ///
    /// The id is usually in [`CuratedTrigger::required_config`]. Notion's
    /// page-created trigger is the exception: Composio leaves its
    /// `parent_page_id` optional, but without one it fires for every page in
    /// the workspace.
    PerResource,
}

/// One entry in the curated trigger catalog.
///
/// Configuration keys are spelled exactly as Composio's trigger config schema
/// spells them, camelCase for some toolkits and `snake_case` for others.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CuratedTrigger {
    /// Composio trigger slug, e.g. `"GMAIL_NEW_GMAIL_MESSAGE"`.
    pub slug: &'static str,
    /// The canonical slug of the toolkit the trigger belongs to, e.g.
    /// `"googlecalendar"`.
    pub toolkit: &'static str,
    /// What the trigger reports.
    pub event: TriggerEvent,
    /// How it is delivered.
    pub delivery: TriggerDelivery,
    /// How early it is worth building on.
    pub tier: TriggerTier,
    /// Configuration keys the trigger cannot be created without.
    pub required_config: &'static [&'static str],
    /// Configuration keys the trigger accepts but does not need.
    pub optional_config: &'static [&'static str],
    /// What the trigger is good for in a personal assistant.
    pub use_case: &'static str,
}

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
