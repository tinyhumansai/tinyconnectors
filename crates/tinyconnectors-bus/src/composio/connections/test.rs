//! Serde and status-normalization tests for the connection payloads.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{ComposioAuthorizeResponse, ComposioConnection};
use serde_json::json;

fn connection_with_status(status: &str) -> ComposioConnection {
    ComposioConnection {
        id: "c1".into(),
        toolkit: "slack".into(),
        status: status.into(),
        created_at: None,
        account_email: None,
        workspace: None,
        username: None,
    }
}

#[test]
fn treats_active_and_connected_as_usable_in_any_casing() {
    for status in ["ACTIVE", "CONNECTED", "active", "connected", " connected "] {
        assert!(
            connection_with_status(status).is_active(),
            "status {status:?} should be active"
        );
    }
}

#[test]
fn rejects_in_flight_and_failed_statuses_as_unusable() {
    for status in ["PENDING", "INITIATED", "FAILED", ""] {
        assert!(
            !connection_with_status(status).is_active(),
            "status {status:?} should not be active"
        );
    }
}

#[test]
fn normalizes_toolkit_for_runtime_matching() {
    let mut conn = connection_with_status("ACTIVE");
    conn.toolkit = " Slack ".into();
    assert_eq!(conn.normalized_toolkit(), "slack");
}

#[test]
fn parses_and_serializes_camel_case_created_at() {
    let raw = json!({
        "id": "conn_1",
        "toolkit": "gmail",
        "status": "ACTIVE",
        "createdAt": "2026-02-01T00:00:00Z"
    });
    let conn: ComposioConnection = serde_json::from_value(raw).expect("parses");
    assert_eq!(conn.id, "conn_1");
    assert_eq!(conn.created_at.as_deref(), Some("2026-02-01T00:00:00Z"));

    let serialized = serde_json::to_value(&conn).expect("serializes");
    assert!(serialized.get("createdAt").is_some());
}

#[test]
fn omits_created_at_when_absent() {
    let conn = connection_with_status("PENDING");
    let value = serde_json::to_value(&conn).expect("serializes");
    assert!(
        value.get("createdAt").is_none(),
        "createdAt must be skipped when None"
    );
}

#[test]
fn authorize_response_uses_camel_case_keys() {
    let raw = json!({
        "connectUrl": "https://composio.dev/oauth/abc",
        "connectionId": "conn_2"
    });
    let resp: ComposioAuthorizeResponse = serde_json::from_value(raw).expect("parses");
    assert_eq!(resp.connect_url, "https://composio.dev/oauth/abc");
    assert_eq!(resp.connection_id, "conn_2");

    let value = serde_json::to_value(&resp).expect("serializes");
    assert!(value.get("connectUrl").is_some());
    assert!(value.get("connectionId").is_some());
}

// ── the configure request ────────────────────────────────────────────

#[test]
fn a_proxy_configuration_is_tagged_exactly_like_the_load_time_blob() {
    // The point of the shared tag is that a host builds one shape and uses it
    // both to load the module and to reconfigure it. A drift here means a host
    // that can load the module but cannot re-route it, which surfaces as a
    // signed-in user who still cannot reach Composio.
    let json = serde_json::to_value(ComposioConfigureRequest::Proxy {
        base_url: "https://api.example.com".to_string(),
        auth_token: "tok".to_string(),
    })
    .expect("serialize");

    assert_eq!(json["route"], "proxy");
    assert_eq!(json["base_url"], "https://api.example.com");
    assert_eq!(json["auth_token"], "tok");
}

#[test]
fn a_direct_configuration_omits_the_optional_fields_it_was_not_given() {
    let json = serde_json::to_value(ComposioConfigureRequest::Direct {
        api_key: "sk-1".to_string(),
        entity_id: None,
        base_url: None,
    })
    .expect("serialize");

    assert_eq!(json["route"], "direct");
    assert_eq!(json["api_key"], "sk-1");
    assert!(json.get("entity_id").is_none());
    assert!(json.get("base_url").is_none());
}

#[test]
fn a_configuration_round_trips_through_the_wire_form() {
    let wire = serde_json::json!({
        "route": "direct",
        "api_key": "sk-2",
        "entity_id": "ent_1",
    });
    let request: ComposioConfigureRequest = serde_json::from_value(wire).expect("decode");
    match request {
        ComposioConfigureRequest::Direct {
            api_key,
            entity_id,
            base_url,
        } => {
            assert_eq!(api_key, "sk-2");
            assert_eq!(entity_id.as_deref(), Some("ent_1"));
            assert!(base_url.is_none());
        }
        ComposioConfigureRequest::Proxy { .. } => panic!("decoded as the wrong route"),
    }
}

#[test]
fn an_unknown_route_name_is_refused_rather_than_defaulted() {
    // Silently picking a route the host did not ask for would send a user's
    // requests somewhere they did not choose.
    let wire = serde_json::json!({ "route": "smtp", "api_key": "sk" });
    assert!(serde_json::from_value::<ComposioConfigureRequest>(wire).is_err());
}

#[test]
fn the_response_names_the_route_now_in_use() {
    let json = serde_json::to_value(ComposioConfigureResponse {
        route: "proxy".to_string(),
    })
    .expect("serialize");
    assert_eq!(json["route"], "proxy");
}
