//! Serde representation tests for the tool schema payloads.

use super::ComposioToolSchema;
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
