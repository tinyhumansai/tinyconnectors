//! Serde representation tests for the execute payload.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::ComposioExecuteResponse;
use serde_json::json;

#[test]
fn parses_cost_and_a_null_error() {
    let raw = json!({
        "data": { "messageId": "m-1" },
        "successful": true,
        "error": null,
        "costUsd": 0.0025
    });
    let resp: ComposioExecuteResponse = serde_json::from_value(raw).expect("parses");
    assert!(resp.successful);
    assert!(resp.error.is_none());
    assert!((resp.cost_usd - 0.0025).abs() < f64::EPSILON);
}

#[test]
fn defaults_to_an_unsuccessful_free_empty_result() {
    let resp: ComposioExecuteResponse = serde_json::from_str("{}").expect("empty object parses");
    assert!(!resp.successful);
    assert!(resp.error.is_none());
    assert!((resp.cost_usd - 0.0).abs() < f64::EPSILON);
    assert!(resp.data.is_null());
    assert!(resp.markdown_formatted.is_none());
}

#[test]
fn keeps_the_camel_case_keys_on_the_wire() {
    let resp = ComposioExecuteResponse {
        cost_usd: 0.5,
        markdown_formatted: Some("**done**".into()),
        ..Default::default()
    };
    let value = serde_json::to_value(&resp).expect("serializes");
    assert!(value.get("costUsd").is_some());
    assert!(value.get("markdownFormatted").is_some());
}
