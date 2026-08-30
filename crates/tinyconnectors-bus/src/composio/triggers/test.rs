//! Serde representation tests for the trigger payloads.
//!
//! The drift-tolerance cases matter more than the happy paths: each of them
//! reproduces a shape Composio has actually sent, and a regression turns a
//! user's live subscriptions into an empty list.

use super::{
    ComposioActiveTrigger, ComposioAvailableTrigger, ComposioDisableTriggerResponse,
    ComposioEnableTriggerResponse, ComposioTriggerEvent,
};
use serde_json::json;

#[test]
fn available_trigger_round_trips_camel_case_fields() {
    let raw = json!({
        "slug": "GMAIL_NEW_GMAIL_MESSAGE",
        "scope": "static",
        "defaultConfig": { "labelIds": ["INBOX"] },
        "requiredConfigKeys": ["labelIds"],
        "repo": { "owner": "acme", "repo": "inbox" }
    });
    let trigger: ComposioAvailableTrigger = serde_json::from_value(raw).expect("parses");
    assert_eq!(trigger.slug, "GMAIL_NEW_GMAIL_MESSAGE");
    assert_eq!(trigger.scope, "static");
    assert_eq!(
        trigger.default_config,
        Some(json!({ "labelIds": ["INBOX"] }))
    );
    assert_eq!(
        trigger.required_config_keys,
        Some(vec!["labelIds".to_string()])
    );
    let repo = trigger.repo.as_ref().expect("repo present");
    assert_eq!(repo.owner, "acme");
    assert_eq!(repo.repo, "inbox");

    let value = serde_json::to_value(&trigger).expect("serializes");
    assert!(value.get("defaultConfig").is_some());
    assert!(value.get("requiredConfigKeys").is_some());
}

#[test]
fn active_trigger_parses_plain_string_fields() {
    let raw = json!({
        "id": "ti_1",
        "slug": "GMAIL_NEW_GMAIL_MESSAGE",
        "toolkit": "gmail",
        "connectionId": "c-1",
        "triggerConfig": { "labelIds": "INBOX" },
        "state": "active"
    });
    let trigger: ComposioActiveTrigger = serde_json::from_value(raw).expect("parses");
    assert_eq!(trigger.id, "ti_1");
    assert_eq!(trigger.connection_id, "c-1");
    assert_eq!(trigger.trigger_config, Some(json!({ "labelIds": "INBOX" })));
    assert_eq!(trigger.state.as_deref(), Some("active"));

    let value = serde_json::to_value(&trigger).expect("serializes");
    assert!(value.get("connectionId").is_some());
    assert!(value.get("triggerConfig").is_some());
    assert!(value.get("state").is_some());
}

#[test]
fn active_trigger_accepts_fields_that_drifted_into_objects() {
    // Mirrors upstream drift where these arrive as objects, not strings.
    let raw = json!({
        "id": { "id": "t1" },
        "slug": { "slug": "GMAIL_NEW_MAIL" },
        "toolkit": { "slug": "gmail", "logo": "https://…" },
        "connectionId": { "id": "c1" },
        "state": { "state": "ACTIVE", "slug": "should-be-ignored" }
    });
    let trigger: ComposioActiveTrigger = serde_json::from_value(raw).expect("parses");
    assert_eq!(trigger.id, "t1");
    assert_eq!(trigger.slug, "GMAIL_NEW_MAIL");
    assert_eq!(trigger.toolkit, "gmail");
    assert_eq!(trigger.connection_id, "c1");
    // A literal `state` key must win over the co-located metadata `slug`.
    assert_eq!(trigger.state.as_deref(), Some("ACTIVE"));
}

#[test]
fn active_trigger_state_falls_back_to_a_value_key() {
    let raw = json!({
        "id": "t1",
        "slug": "X",
        "toolkit": "gmail",
        "connectionId": "c1",
        "state": { "value": "PENDING" }
    });
    let trigger: ComposioActiveTrigger = serde_json::from_value(raw).expect("parses");
    assert_eq!(trigger.state.as_deref(), Some("PENDING"));
}

#[test]
fn active_trigger_state_is_none_when_missing_or_unrecognized() {
    let base = json!({
        "id": "t1",
        "slug": "X",
        "toolkit": "gmail",
        "connectionId": "c1"
    });
    let trigger: ComposioActiveTrigger = serde_json::from_value(base.clone()).expect("parses");
    assert!(trigger.state.is_none());

    let mut with_junk = base;
    with_junk["state"] = json!({ "unrelated": 42 });
    let trigger: ComposioActiveTrigger = serde_json::from_value(with_junk).expect("parses");
    assert!(trigger.state.is_none());
}

#[test]
fn active_trigger_rejects_a_required_field_it_cannot_read() {
    // An object carrying none of slug/id/name/key must fail loudly, so further
    // drift surfaces as a decode error instead of a silently dropped trigger.
    let raw = json!({
        "id": { "unrelated": 42 },
        "slug": "X",
        "toolkit": "gmail",
        "connectionId": "c1"
    });
    let err = serde_json::from_value::<ComposioActiveTrigger>(raw).expect_err("must reject");
    assert!(err.to_string().contains("expected string or object"));
}

#[test]
fn enable_response_uses_camel_case_keys() {
    let raw = json!({
        "triggerId": "ti_9",
        "slug": "GMAIL_NEW_GMAIL_MESSAGE",
        "connectionId": "c-9"
    });
    let resp: ComposioEnableTriggerResponse = serde_json::from_value(raw).expect("parses");
    assert_eq!(resp.trigger_id, "ti_9");
    assert_eq!(resp.connection_id, "c-9");

    let value = serde_json::to_value(&resp).expect("serializes");
    assert_eq!(value["triggerId"], "ti_9");
    assert_eq!(value["connectionId"], "c-9");
}

#[test]
fn disable_response_defaults_deleted_to_false() {
    let resp: ComposioDisableTriggerResponse =
        serde_json::from_value(json!({})).expect("empty object parses");
    assert!(!resp.deleted);
}

#[test]
fn trigger_event_defaults_every_field() {
    let event: ComposioTriggerEvent = serde_json::from_str("{}").expect("empty object parses");
    assert_eq!(event.toolkit, "");
    assert_eq!(event.trigger, "");
    assert_eq!(event.metadata.id, "");
    assert_eq!(event.metadata.uuid, "");
    assert!(event.payload.is_null());
}

#[test]
fn trigger_event_parses_a_full_delivery() {
    let raw = json!({
        "toolkit": "gmail",
        "trigger": "GMAIL_NEW_GMAIL_MESSAGE",
        "payload": { "subject": "hi" },
        "metadata": { "id": "evt-1", "uuid": "uuid-1" }
    });
    let event: ComposioTriggerEvent = serde_json::from_value(raw).expect("parses");
    assert_eq!(event.toolkit, "gmail");
    assert_eq!(event.trigger, "GMAIL_NEW_GMAIL_MESSAGE");
    assert_eq!(event.metadata.id, "evt-1");
    assert_eq!(event.metadata.uuid, "uuid-1");
    assert_eq!(event.payload["subject"], "hi");
}
