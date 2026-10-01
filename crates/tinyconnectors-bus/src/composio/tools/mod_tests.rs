//! Serde representation tests for the tool schema payloads.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{
    ComposioGetUserScopesRequest, ComposioListToolsRequest, ComposioSetUserScopesRequest,
    ComposioToolSchema, ComposioUserScopes, ComposioUserScopesResponse,
};
use serde_json::json;

#[test]
fn defaults_the_envelope_type_to_function() {
    let raw = json!({
        "function": {
            "name": "GMAIL_SEND_EMAIL",
            "description": "Send an email",
            "parameters": { "type": "object" }
        }
    });
    let tool: ComposioToolSchema = serde_json::from_value(raw).expect("parses");
    assert_eq!(tool.kind, "function");
    assert_eq!(tool.function.name, "GMAIL_SEND_EMAIL");
    assert_eq!(tool.function.description.as_deref(), Some("Send an email"));
    assert!(tool.function.parameters.is_some());
}

#[test]
fn tolerates_a_name_only_function() {
    let raw = json!({ "function": { "name": "SLUG_ONLY" } });
    let tool: ComposioToolSchema = serde_json::from_value(raw).expect("parses");
    assert_eq!(tool.function.name, "SLUG_ONLY");
    assert!(tool.function.description.is_none());
    assert!(tool.function.parameters.is_none());
    assert!(tool.function.output_parameters.is_none());
}

#[test]
fn keeps_the_output_schema_when_upstream_publishes_one() {
    let raw = json!({
        "function": {
            "name": "GMAIL_FETCH_EMAILS",
            "output_parameters": { "type": "object" }
        }
    });
    let tool: ComposioToolSchema = serde_json::from_value(raw).expect("parses");
    assert!(tool.function.output_parameters.is_some());

    let value = serde_json::to_value(&tool).expect("serializes");
    assert!(value["function"].get("output_parameters").is_some());
}

#[test]
fn a_tool_listing_applies_the_user_scopes_by_default() {
    // A listing is what an agent picks from, so the safe default is the
    // filtered one: showing an action that will then be refused wastes a turn.
    let request = ComposioListToolsRequest::default();
    assert!(request.apply_user_scopes);
    assert!(request.toolkits.is_empty());

    let parsed: ComposioListToolsRequest =
        serde_json::from_value(json!({ "toolkits": ["gmail"] })).expect("parses");
    assert!(parsed.apply_user_scopes, "an absent flag means filtered");
}

#[test]
fn a_tool_listing_omits_its_empty_filters_on_the_wire() {
    let value = serde_json::to_value(ComposioListToolsRequest::default()).expect("serializes");
    assert!(value.get("toolkits").is_none());
    assert!(value.get("tags").is_none());
    assert_eq!(value["apply_user_scopes"], true);
}

#[test]
fn the_default_scopes_allow_reads_and_writes_but_not_admin() {
    // Read alone makes most integrations useless; admin is the set that
    // destroys things, so a user should have to ask for it.
    let scopes = ComposioUserScopes::default();
    assert!(scopes.read);
    assert!(scopes.write);
    assert!(!scopes.admin);
}

#[test]
fn a_partial_scope_row_fills_in_the_defaults() {
    // A row written before a flag existed must not read as denying it.
    let scopes: ComposioUserScopes =
        serde_json::from_value(json!({ "admin": true })).expect("parses");
    assert!(scopes.read);
    assert!(scopes.write);
    assert!(scopes.admin);
}

#[test]
fn the_scope_requests_round_trip_flattened() {
    let request: ComposioSetUserScopesRequest = serde_json::from_value(json!({
        "toolkit": "gmail", "read": true, "write": false, "admin": false
    }))
    .expect("parses");
    assert_eq!(request.toolkit, "gmail");
    assert!(!request.scopes.write);

    // Flattened on the way out too, so the wire shape is one flat object.
    let value = serde_json::to_value(&request).expect("serializes");
    assert_eq!(value["write"], false);
    assert!(value.get("scopes").is_none());

    let response = ComposioUserScopesResponse {
        toolkit: "gmail".into(),
        scopes: ComposioUserScopes::default(),
    };
    let value = serde_json::to_value(&response).expect("serializes");
    assert_eq!(value["toolkit"], "gmail");
    assert_eq!(value["admin"], false);

    let get: ComposioGetUserScopesRequest =
        serde_json::from_value(json!({ "toolkit": "notion" })).expect("parses");
    assert_eq!(get.toolkit, "notion");
}
