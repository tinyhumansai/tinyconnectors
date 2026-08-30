//! Serde representation tests for the toolkit payloads.
//!
//! These pin the wire form. A field rename that compiles fine here would fail
//! at runtime as a decode error in a host, so the assertions check the JSON
//! keys, not just the round-trip.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{ComposioToolkitCatalogEntry, ComposioToolkitsResponse};
use serde_json::json;

#[test]
fn toolkits_response_defaults_to_empty() {
    let resp: ComposioToolkitsResponse = serde_json::from_str("{}").expect("empty object parses");
    assert!(resp.toolkits.is_empty());
    assert!(resp.catalog.is_empty());
}

#[test]
fn toolkits_response_skips_an_empty_catalog_on_the_wire() {
    let resp = ComposioToolkitsResponse {
        toolkits: vec!["gmail".into(), "notion".into()],
        ..Default::default()
    };
    let value = serde_json::to_value(&resp).expect("serializes");
    // Back-compat with cores that predate the dynamic catalog: they must not
    // see a `catalog` key at all.
    assert_eq!(value, json!({ "toolkits": ["gmail", "notion"] }));

    let back: ComposioToolkitsResponse = serde_json::from_value(value).expect("round-trips");
    assert_eq!(back.toolkits, vec!["gmail", "notion"]);
    assert!(back.catalog.is_empty());
}

#[test]
fn toolkits_response_forwards_the_catalog_verbatim() {
    let raw = json!({
        "toolkits": ["gmail"],
        "catalog": [{
            "slug": "gmail",
            "name": "Gmail",
            "logo": "https://logos.composio.dev/api/gmail",
            "description": "Send and read email",
            "categories": ["productivity"],
            "enabled": true
        }]
    });
    let resp: ComposioToolkitsResponse = serde_json::from_value(raw).expect("parses");
    assert_eq!(resp.catalog.len(), 1);
    let entry = &resp.catalog[0];
    assert_eq!(entry.slug, "gmail");
    assert_eq!(entry.name, "Gmail");
    assert_eq!(entry.enabled, Some(true));
    assert_eq!(entry.categories, vec!["productivity".to_string()]);

    let value = serde_json::to_value(&resp).expect("serializes");
    assert_eq!(value["catalog"][0]["slug"], "gmail");
    assert_eq!(value["catalog"][0]["enabled"], true);
}

#[test]
fn catalog_entry_tolerates_a_slug_only_row() {
    let entry: ComposioToolkitCatalogEntry =
        serde_json::from_value(json!({ "slug": "notion" })).expect("parses");
    assert_eq!(entry.slug, "notion");
    assert!(entry.name.is_empty());
    assert!(entry.logo.is_none());
    assert!(entry.categories.is_empty());
    assert!(entry.enabled.is_none());
}
