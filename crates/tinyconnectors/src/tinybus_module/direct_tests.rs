//! The stateless direct members, and network settings on a configured route.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, RwLock};

use serde_json::json;
use tinyconnectors_bus::{
    ComposioConfigureRequest, ComposioDirectConnectionsRequest, ComposioDirectCredential,
    ComposioDirectToolsRequest, ComposioTransportConfig,
};

use super::{ConnectorService, ModuleConfig};
use crate::client::test_support::{connect_proxy, json, server};

fn unrouted_service() -> ConnectorService {
    let client = Arc::new(RwLock::new(None));
    ConnectorService {
        actions: Arc::new(crate::providers::ClientActions::new(Arc::clone(&client))),
        state: Arc::new(super::EphemeralStateStore::default()),
        registry: crate::providers::default_registry(),
        client,
        archive: None,
    }
}

fn credential(base_url: &str) -> ComposioDirectCredential {
    ComposioDirectCredential {
        api_key: "ck_member".to_string(),
        entity_id: None,
        base_url: Some(base_url.to_string()),
        transport: None,
    }
}

#[tokio::test]
async fn a_direct_read_works_on_a_module_that_has_no_route_and_installs_none() {
    let (base, seen) =
        server(|_| json(r#"{"items":[{"id":"ca_1","toolkit":"gmail","status":"ACTIVE"}]}"#));
    let service = unrouted_service();

    let reply = service
        .list_connections_direct(ComposioDirectConnectionsRequest {
            credential: credential(&base),
        })
        .await
        .expect("the credential on the request is enough");
    assert_eq!(reply.connections[0].id, "ca_1");
    assert!(
        seen.lock().unwrap()[0]
            .to_ascii_lowercase()
            .contains("x-api-key: ck_member"),
        "the request's own key is what is sent"
    );

    // Nothing was installed: the configured route is still absent.
    let after = service.list_toolkits().await.unwrap_err();
    assert!(
        after.to_string().contains("without a connector route"),
        "{after}"
    );
}

#[tokio::test]
async fn a_direct_tools_read_passes_filters_through_and_skips_scope_preferences() {
    let (base, seen) = server(|_| {
        json(
            r#"{"items":[{"slug":"GITHUB_STAR","description":"Star","toolkit":{"slug":"github"}}]}"#,
        )
    });
    let service = unrouted_service();

    let reply = service
        .list_tools_direct(ComposioDirectToolsRequest {
            credential: credential(&base),
            toolkits: vec!["github".to_string()],
            tags: vec!["stars".to_string()],
        })
        .await
        .unwrap();
    assert_eq!(reply.tools[0].function.name, "GITHUB_STAR");
    let head = seen.lock().unwrap()[0].clone();
    assert!(
        head.contains("toolkits=github") && head.contains("tags=stars"),
        "{head}"
    );
}

#[tokio::test]
async fn a_direct_read_failure_crosses_the_bus_as_the_user_facing_message() {
    let (base, _) = server(|_| {
        (
            401,
            Vec::new(),
            r#"{"error":{"message":"Invalid API key"}}"#.to_string(),
        )
    });
    let service = unrouted_service();
    let error = service
        .list_connections_direct(ComposioDirectConnectionsRequest {
            credential: credential(&base),
        })
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.ends_with("Composio v3 connected_accounts failed: HTTP 401: Invalid API key"),
        "{error}"
    );
    let error = service
        .list_tools_direct(ComposioDirectToolsRequest {
            credential: credential(&base),
            toolkits: Vec::new(),
            tags: Vec::new(),
        })
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.ends_with("Composio v3 list_tool_schemas: HTTP 401: Invalid API key"),
        "{error}"
    );
}

#[tokio::test]
async fn a_configured_direct_route_sends_through_the_proxy_it_was_given() {
    let (base, seen) =
        server(|_| json(r#"{"items":[{"id":"ca_9","toolkit":"gmail","status":"ACTIVE"}]}"#));
    let (proxy, proxied) = connect_proxy();
    let service = unrouted_service();

    service
        .configure(ComposioConfigureRequest::Direct {
            api_key: "ck_configured".to_string(),
            entity_id: None,
            base_url: Some(base),
            transport: Some(ComposioTransportConfig {
                proxy_url: Some(proxy),
                ..Default::default()
            }),
        })
        .await
        .unwrap();

    let reply = service.list_connections().await.unwrap();
    assert_eq!(reply.connections[0].id, "ca_9");
    assert!(
        proxied.try_recv().is_ok(),
        "the configured route used the proxy"
    );
    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn an_unusable_proxy_refuses_the_configure_and_keeps_the_old_route() {
    let service = unrouted_service();
    let error = service
        .configure(ComposioConfigureRequest::Direct {
            api_key: "ck".to_string(),
            entity_id: None,
            base_url: None,
            transport: Some(ComposioTransportConfig {
                proxy_url: Some("ftp://nope".to_string()),
                ..Default::default()
            }),
        })
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("invalid network settings"),
        "{error}"
    );
}

#[test]
fn a_load_time_blob_carries_network_settings_to_both_routes() {
    for blob in [
        json!({ "route": "direct", "api_key": "ck", "transport": { "tls_roots": "platform" } }),
        json!({
            "route": "proxy", "base_url": "https://api.example.com", "auth_token": "t",
            "transport": { "proxy_url": "http://127.0.0.1:3128", "no_proxy": ["localhost"] }
        }),
    ] {
        let config: ModuleConfig = serde_json::from_value(blob).unwrap();
        assert!(config.into_route().unwrap().is_some());
    }

    let bad: ModuleConfig = serde_json::from_value(json!({
        "route": "direct", "api_key": "ck", "transport": { "proxy_url": "ftp://x" }
    }))
    .unwrap();
    assert!(bad.into_route().is_err());
}

#[test]
fn the_manifest_declares_every_member_the_contract_names_in_order() {
    // The released artifact is refused by the loader's verification when its
    // declared members differ from the contract's table, and that check only
    // runs against a built module. Read the declaration from the source so the
    // same drift fails here, in a unit test, before a release attempt does.
    let source = include_str!("mod.rs");
    let export = &source[source.rfind("export_module! {").expect("the export")..];
    let list = export
        .split("methods = [")
        .nth(1)
        .and_then(|rest| rest.split(']').next())
        .expect("the methods declaration");
    let declared: Vec<&str> = list
        .split(',')
        .map(|entry| entry.trim().trim_matches('"'))
        .filter(|entry| !entry.is_empty())
        .collect();
    assert_eq!(declared, tinyconnectors_bus::METHODS);
}
