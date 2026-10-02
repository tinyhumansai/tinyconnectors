//! Unit tests for the Google Calendar default-arguments transformer.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::json;

use super::{TZ_DEFAULTING_SLUGS, apply_calendar_query_defaults};

#[test]
fn injects_tz_and_single_events_when_absent() {
    let args = apply_calendar_query_defaults(
        "GOOGLECALENDAR_EVENTS_LIST",
        Some(json!({
            "connectionId": "conn-1",
            "timeMin": "2026-05-14T00:00:00+05:30",
            "timeMax": "2026-05-14T23:59:59+05:30",
            "maxResults": 20
        })),
        "Asia/Kolkata",
    )
    .expect("transformer must return a payload");

    assert_eq!(args["timeZone"], "Asia/Kolkata");
    assert_eq!(args["singleEvents"], true);
    assert_eq!(
        args["connectionId"], "conn-1",
        "must not perturb caller-supplied fields"
    );
}

#[test]
fn does_not_overwrite_caller_supplied_time_zone() {
    let args = apply_calendar_query_defaults(
        "GOOGLECALENDAR_EVENTS_LIST",
        Some(json!({
            "timeZone": "America/Los_Angeles",
            "timeMin": "2026-05-14T00:00:00-07:00",
        })),
        "Asia/Kolkata",
    )
    .unwrap();

    assert_eq!(args["timeZone"], "America/Los_Angeles");
    assert_eq!(args["singleEvents"], true);
}

#[test]
fn does_not_overwrite_caller_supplied_single_events() {
    let args = apply_calendar_query_defaults(
        "GOOGLECALENDAR_FIND_EVENT",
        Some(json!({ "singleEvents": false })),
        "UTC",
    )
    .unwrap();

    assert_eq!(args["singleEvents"], false);
    assert_eq!(args["timeZone"], "UTC");
}

#[test]
fn absent_arguments_become_an_object_with_defaults() {
    let args = apply_calendar_query_defaults("GOOGLECALENDAR_EVENTS_LIST", None, "Europe/London")
        .expect("absent arguments must coerce to a populated object");

    assert!(args.is_object());
    assert_eq!(args["timeZone"], "Europe/London");
    assert_eq!(args["singleEvents"], true);
}

#[test]
fn other_actions_are_untouched() {
    let original = json!({ "to": "alice@example.com", "subject": "hi" });
    let passed =
        apply_calendar_query_defaults("GMAIL_SEND_EMAIL", Some(original.clone()), "Asia/Kolkata")
            .unwrap();
    assert_eq!(passed, original);
    assert_eq!(
        apply_calendar_query_defaults("GMAIL_SEND_EMAIL", None, "UTC"),
        None
    );
}

#[test]
fn non_object_payload_is_untouched() {
    let passed = apply_calendar_query_defaults(
        "GOOGLECALENDAR_EVENTS_LIST",
        Some(json!(["unexpected"])),
        "Asia/Kolkata",
    )
    .unwrap();
    assert_eq!(passed, json!(["unexpected"]));
}

#[test]
fn allowlist_covers_the_window_taking_actions() {
    assert!(TZ_DEFAULTING_SLUGS.contains(&"GOOGLECALENDAR_EVENTS_LIST"));
    assert!(TZ_DEFAULTING_SLUGS.contains(&"GOOGLECALENDAR_FIND_EVENT"));
}
