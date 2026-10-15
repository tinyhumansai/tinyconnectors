//! Pins the curated trigger tables and the lookups over them.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeSet;

use super::*;
use crate::toolkits::catalog::canonical_toolkit;

/// Every event, with the wire name a host persists and sends for it.
pub(super) const EVENT_WIRE_NAMES: &[(TriggerEvent, &str)] = &[
    (TriggerEvent::NewEmail, "new_email"),
    (TriggerEvent::EmailSent, "email_sent"),
    (TriggerEvent::MeetingStartingSoon, "meeting_starting_soon"),
    (TriggerEvent::CalendarEventCreated, "calendar_event_created"),
    (
        TriggerEvent::CalendarEventCanceled,
        "calendar_event_canceled",
    ),
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

/// Per-resource triggers whose resource key Composio lists as optional.
const OPTIONAL_RESOURCE_KEYS: &[(&str, &str)] = &[("NOTION_PAGE_CREATED", "parent_page_id")];

fn slugs<'a>(triggers: impl Iterator<Item = &'a CuratedTrigger>) -> Vec<&'a str> {
    triggers.map(|t| t.slug).collect()
}

#[test]
fn tables_match_the_verified_fixture() {
    // The fixture is a separate transcription of the list verified against
    // Composio; it pins every slug, toolkit, event, delivery, tier and key.
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/curated-triggers.json")).unwrap();
    let mut actual: Vec<serde_json::Value> = curated_triggers()
        .map(|t| {
            serde_json::json!({
                "slug": t.slug,
                "toolkit": t.toolkit,
                "event": t.event,
                "delivery": t.delivery,
                "tier": t.tier,
                "required_config": t.required_config,
                "optional_config": t.optional_config,
            })
        })
        .collect();
    actual.sort_by(|a, b| a["slug"].as_str().cmp(&b["slug"].as_str()));
    assert_eq!(serde_json::Value::Array(actual), expected);
}

#[test]
fn every_slug_starts_with_its_canonical_toolkit() {
    for trigger in curated_triggers() {
        assert_eq!(canonical_toolkit(trigger.toolkit), trigger.toolkit);
        let prefix = format!("{}_", trigger.toolkit.to_ascii_uppercase());
        assert!(
            trigger.slug.starts_with(&prefix),
            "{} is not a {} trigger",
            trigger.slug,
            trigger.toolkit
        );
    }
}

#[test]
fn slugs_are_unique() {
    let all = slugs(curated_triggers());
    let distinct: BTreeSet<&str> = all.iter().copied().collect();
    assert_eq!(all.len(), distinct.len());
}

#[test]
fn each_toolkit_has_at_most_one_trigger_per_event() {
    // `trigger_for_event` returns the first match, so a second row for the
    // same pair would be unreachable.
    let mut seen = BTreeSet::new();
    for trigger in curated_triggers() {
        assert!(
            seen.insert((trigger.toolkit, trigger.event.as_str())),
            "{} repeats {:?} on {}",
            trigger.slug,
            trigger.event,
            trigger.toolkit
        );
    }
}

#[test]
fn every_event_has_at_least_one_trigger() {
    for (event, _) in EVENT_WIRE_NAMES {
        assert!(
            toolkits_for_event(*event).next().is_some(),
            "no trigger reports {event:?}"
        );
    }
}

#[test]
fn only_per_resource_triggers_require_config() {
    for trigger in curated_triggers() {
        match trigger.tier {
            TriggerTier::PerResource => {
                if let Some((_, key)) = OPTIONAL_RESOURCE_KEYS
                    .iter()
                    .find(|(slug, _)| *slug == trigger.slug)
                {
                    assert!(trigger.optional_config.contains(key), "{}", trigger.slug);
                } else {
                    assert!(!trigger.required_config.is_empty(), "{}", trigger.slug);
                }
            }
            TriggerTier::Core | TriggerTier::Extended => {
                assert!(trigger.required_config.is_empty(), "{}", trigger.slug);
            }
        }
    }
}

#[test]
fn poll_triggers_take_an_interval_and_webhooks_do_not() {
    for trigger in curated_triggers() {
        let has_interval = trigger.optional_config.contains(&"interval");
        assert_eq!(
            has_interval,
            trigger.delivery == TriggerDelivery::Poll,
            "{}",
            trigger.slug
        );
    }
}

#[test]
fn every_trigger_explains_its_use() {
    for trigger in curated_triggers() {
        assert!(!trigger.use_case.trim().is_empty(), "{}", trigger.slug);
    }
}

#[test]
fn curated_trigger_finds_a_slug_case_insensitively() {
    let found = curated_trigger("  gmail_new_gmail_message ").expect("curated");
    assert_eq!(found.slug, "GMAIL_NEW_GMAIL_MESSAGE");
    assert_eq!(found.event, TriggerEvent::NewEmail);
    assert!(curated_trigger("GMAIL_SEND_EMAIL").is_none());
    assert!(curated_trigger("").is_none());
}

#[test]
fn curated_triggers_for_toolkit_folds_aliases_and_case() {
    let calendar = slugs(curated_triggers_for_toolkit("google_calendar"));
    assert_eq!(
        calendar,
        [
            "GOOGLECALENDAR_EVENT_STARTING_SOON_TRIGGER",
            "GOOGLECALENDAR_GOOGLE_CALENDAR_EVENT_CREATED_TRIGGER",
            "GOOGLECALENDAR_EVENT_CANCELED_DELETED_TRIGGER",
            "GOOGLECALENDAR_ATTENDEE_RESPONSE_CHANGED_TRIGGER",
        ]
    );
    assert_eq!(
        slugs(curated_triggers_for_toolkit(" GoogleCalendar ")),
        calendar
    );
    assert_eq!(curated_triggers_for_toolkit("SLACK").count(), 3);
    assert_eq!(curated_triggers_for_toolkit("jira").count(), 0);
}

#[test]
fn trigger_for_event_picks_the_connected_toolkits_trigger() {
    assert_eq!(
        trigger_for_event(TriggerEvent::NewEmail, "outlook").map(|t| t.slug),
        Some("OUTLOOK_MESSAGE_TRIGGER")
    );
    assert_eq!(
        trigger_for_event(TriggerEvent::MeetingStartingSoon, "googlesuper").map(|t| t.slug),
        Some("GOOGLESUPER_EVENT_STARTING_SOON_TRIGGER")
    );
    assert_eq!(
        trigger_for_event(TriggerEvent::MeetingStartingSoon, "Google_Calendar").map(|t| t.slug),
        Some("GOOGLECALENDAR_EVENT_STARTING_SOON_TRIGGER")
    );
    assert!(trigger_for_event(TriggerEvent::DirectMessage, "gmail").is_none());
    assert!(trigger_for_event(TriggerEvent::NewEmail, "nonexistent").is_none());
}

#[test]
fn toolkits_for_event_lists_each_toolkit_once_in_table_order() {
    assert_eq!(
        toolkits_for_event(TriggerEvent::NewEmail).collect::<Vec<_>>(),
        ["gmail", "googlesuper", "outlook"]
    );
    assert_eq!(
        toolkits_for_event(TriggerEvent::MeetingStartingSoon).collect::<Vec<_>>(),
        ["googlecalendar", "googlesuper"]
    );
    assert_eq!(
        toolkits_for_event(TriggerEvent::PullRequestActivity).collect::<Vec<_>>(),
        ["github"]
    );
}
