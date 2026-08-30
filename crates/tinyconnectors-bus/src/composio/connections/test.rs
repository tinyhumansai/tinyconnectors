//! Serde and status-normalization tests for the connection payloads.

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
