//! Serde representation tests for the direct-read payloads.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{
    ComposioDirectConnectionsRequest, ComposioDirectCredential, ComposioDirectToolsRequest,
    ComposioTlsRoots, ComposioTransportConfig,
};
use serde_json::json;

#[test]
fn an_empty_transport_config_is_the_historical_behaviour() {
    let config: ComposioTransportConfig = serde_json::from_value(json!({})).unwrap();
    assert_eq!(config, ComposioTransportConfig::default());
    assert_eq!(config.tls_roots, ComposioTlsRoots::Bundled);
    // And it adds nothing to the wire, so an older module sees the same blob.
    assert_eq!(serde_json::to_value(&config).unwrap(), json!({}));
}

#[test]
fn a_full_transport_config_round_trips_with_snake_case_roots() {
    let wire = json!({
        "proxy_url": "http://127.0.0.1:8080",
        "no_proxy": ["localhost", ".internal"],
        "tls_roots": "platform"
    });
    let config: ComposioTransportConfig = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(config.tls_roots, ComposioTlsRoots::Platform);
    assert_eq!(config.no_proxy, ["localhost", ".internal"]);
    assert_eq!(serde_json::to_value(&config).unwrap(), wire);
}

#[test]
fn debug_never_prints_the_proxy_url_or_the_key() {
    let credential = ComposioDirectCredential {
        api_key: "ck_secret_key".to_string(),
        entity_id: None,
        base_url: None,
        transport: Some(ComposioTransportConfig {
            proxy_url: Some("http://user:hunter2@proxy.internal:3128".to_string()),
            ..Default::default()
        }),
    };
    let rendered = format!("{credential:?}");
    assert!(!rendered.contains("ck_secret_key"), "{rendered}");
    assert!(!rendered.contains("hunter2"), "{rendered}");
    assert!(!rendered.contains("proxy.internal"), "{rendered}");
}

#[test]
fn a_credential_needs_only_a_key() {
    let credential: ComposioDirectCredential =
        serde_json::from_value(json!({ "api_key": "ck" })).unwrap();
    assert_eq!(credential.api_key, "ck");
    assert!(credential.entity_id.is_none());
    assert!(credential.base_url.is_none());
    assert!(credential.transport.is_none());
}

#[test]
fn the_requests_keep_their_wire_shape() {
    let connections = ComposioDirectConnectionsRequest {
        credential: ComposioDirectCredential {
            api_key: "ck".to_string(),
            entity_id: Some("ent".to_string()),
            base_url: Some("http://127.0.0.1:1/api".to_string()),
            transport: None,
        },
    };
    let value = serde_json::to_value(&connections).unwrap();
    assert_eq!(value["credential"]["api_key"], "ck");
    assert_eq!(value["credential"]["entity_id"], "ent");
    assert!(value["credential"].get("transport").is_none());

    let tools: ComposioDirectToolsRequest = serde_json::from_value(json!({
        "credential": { "api_key": "ck" },
        "toolkits": ["github"],
        "tags": ["stars"]
    }))
    .unwrap();
    assert_eq!(tools.toolkits, ["github"]);
    assert_eq!(tools.tags, ["stars"]);

    let bare: ComposioDirectToolsRequest =
        serde_json::from_value(json!({ "credential": { "api_key": "ck" } })).unwrap();
    assert!(bare.toolkits.is_empty() && bare.tags.is_empty());
}
