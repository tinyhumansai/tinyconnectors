//! Pins the wire form of the trigger catalog's enums.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use super::*;

/// Every event, with the wire name a host persists and sends for it.
pub(super) const EVENT_WIRE_NAMES: &[(TriggerEvent, &str)] = &[
    (TriggerEvent::NewEmail, "new_email"),
    (TriggerEvent::EmailSent, "email_sent"),
    (TriggerEvent::MeetingStartingSoon, "meeting_starting_soon"),
    (TriggerEvent::CalendarEventCreated, "calendar_event_created"),
    (TriggerEvent::CalendarEventCanceled, "calendar_event_canceled"),
    (
        TriggerEvent::AttendeeResponseChanged,
        "attendee_response_changed",
    ),
    (TriggerEvent::DirectMessage, "direct_message"),
    (TriggerEvent::ChannelMessage, "channel_message"),
    (TriggerEvent::ReactionAdded, "reaction_added"),
    (TriggerEvent::FileShared, "file_shared"),
    (TriggerEvent::DocumentComment, "document_comment"),
    (TriggerEvent::PageCreated, "page_created"),
    (TriggerEvent::PageComment, "page_comment"),
    (TriggerEvent::IssueAssignedToMe, "issue_assigned_to_me"),
    (TriggerEvent::IssueCreated, "issue_created"),
    (TriggerEvent::IssueUpdated, "issue_updated"),
    (TriggerEvent::IssueComment, "issue_comment"),
    (TriggerEvent::PullRequestActivity, "pull_request_activity"),
    (
        TriggerEvent::PullRequestReviewSubmitted,
        "pull_request_review_submitted",
    ),
    (
        TriggerEvent::RepositoryNotification,
        "repository_notification",
    ),
];

/// Fails to compile when a variant is added without a row in
/// [`EVENT_WIRE_NAMES`], so the pin cannot silently fall behind the enum.
#[allow(dead_code)]
fn every_event_has_a_wire_name_row(event: TriggerEvent) {
    match event {
        TriggerEvent::NewEmail
        | TriggerEvent::EmailSent
        | TriggerEvent::MeetingStartingSoon
        | TriggerEvent::CalendarEventCreated
        | TriggerEvent::CalendarEventCanceled
        | TriggerEvent::AttendeeResponseChanged
        | TriggerEvent::DirectMessage
        | TriggerEvent::ChannelMessage
        | TriggerEvent::ReactionAdded
        | TriggerEvent::FileShared
        | TriggerEvent::DocumentComment
        | TriggerEvent::PageCreated
        | TriggerEvent::PageComment
        | TriggerEvent::IssueAssignedToMe
        | TriggerEvent::IssueCreated
        | TriggerEvent::IssueUpdated
        | TriggerEvent::IssueComment
        | TriggerEvent::PullRequestActivity
        | TriggerEvent::PullRequestReviewSubmitted
        | TriggerEvent::RepositoryNotification => {}
    }
}

#[test]
fn trigger_event_wire_form_is_snake_case_and_matches_as_str() {
    for (event, wire) in EVENT_WIRE_NAMES {
        assert_eq!(event.as_str(), *wire);
        assert_eq!(
            serde_json::to_value(event).unwrap(),
            serde_json::json!(wire)
        );
        let decoded: TriggerEvent = serde_json::from_value(serde_json::json!(wire)).unwrap();
        assert_eq!(decoded, *event);
    }
}

#[test]
fn trigger_event_rejects_an_unknown_wire_name() {
    assert!(serde_json::from_value::<TriggerEvent>(serde_json::json!("NewEmail")).is_err());
}

#[test]
fn trigger_delivery_wire_form_is_pinned() {
    for (delivery, wire) in [
        (TriggerDelivery::Webhook, "webhook"),
        (TriggerDelivery::Poll, "poll"),
    ] {
        assert_eq!(
            serde_json::to_value(delivery).unwrap(),
            serde_json::json!(wire)
        );
        let decoded: TriggerDelivery = serde_json::from_value(serde_json::json!(wire)).unwrap();
        assert_eq!(decoded, delivery);
    }
}

#[test]
fn trigger_tier_wire_form_is_pinned() {
    for (tier, wire) in [
        (TriggerTier::Core, "core"),
        (TriggerTier::Extended, "extended"),
        (TriggerTier::PerResource, "per_resource"),
    ] {
        assert_eq!(serde_json::to_value(tier).unwrap(), serde_json::json!(wire));
        let decoded: TriggerTier = serde_json::from_value(serde_json::json!(wire)).unwrap();
        assert_eq!(decoded, tier);
    }
}
