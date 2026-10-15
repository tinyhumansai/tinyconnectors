//! Pins the wire form of the trigger catalog's enums.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use super::*;
use crate::triggers::tests::EVENT_WIRE_NAMES;

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
