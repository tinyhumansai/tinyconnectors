//! Tests for the `TinyBus` module adapter and its declared surface.
//!
//! The bus tests run the real in-memory broker against a stub transport, so
//! they exercise the whole path — frame in, client call, envelope out — without
//! a network. What they are checking is the boundary, not the client: that
//! arguments survive serialization, that responses come back in the contract's
//! shape, and that a failure reaches the caller as a message it can act on.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;
use tinybus::broker::Broker;
use tinybus::transport::memory::MemoryBus;
use tinybus::{Connection, Interface};
use tinyconnectors_bus::{
    ComposioAuthorizeRequest, ComposioAuthorizeResponse, ComposioConnectionsResponse,
    ComposioDeleteConnectionRequest, ComposioDeleteResponse, ComposioExecuteRequest,
    ComposioExecuteResponse, ComposioListToolsRequest, ComposioToolkitsResponse,
    ComposioToolsResponse, names,
};

use super::{ConnectorService, ModuleConfig};
use crate::client::{ComposioClient, ProxyRoute, Transport};
use crate::{Error, Result};

#[derive(Debug, Default)]
struct StubTransport {
    reply: Mutex<serde_json::Value>,
    fail: Mutex<Option<String>>,
    last_body: Mutex<Option<serde_json::Value>>,
}

impl StubTransport {
    fn replying(value: serde_json::Value) -> Arc<Self> {
        Arc::new(Self {
            reply: Mutex::new(value),
            ..Self::default()
        })
    }

    fn failing(message: &str) -> Arc<Self> {
        Arc::new(Self {
            fail: Mutex::new(Some(message.to_string())),
            ..Self::default()
        })
    }

    fn answer(&self, path: &str) -> Result<serde_json::Value> {
        if let Some(message) = self.fail.lock().unwrap().clone() {
            return Err(Error::Transport {
                path: path.to_string(),
                message,
            });
        }
        Ok(self.reply.lock().unwrap().clone())
    }
}

#[async_trait]
impl Transport for StubTransport {
    async fn get(&self, path: &str) -> Result<serde_json::Value> {
        self.answer(path)
    }

    async fn post(&self, path: &str, body: &serde_json::Value) -> Result<serde_json::Value> {
        *self.last_body.lock().unwrap() = Some(body.clone());
        self.answer(path)
    }

    async fn delete(&self, path: &str) -> Result<serde_json::Value> {
        self.answer(path)
    }
}

fn service_over(transport: Arc<StubTransport>) -> ConnectorService {
    let client = ComposioClient::new(Arc::new(ProxyRoute::new(transport)));
    ConnectorService {
        actions: Arc::new(crate::providers::ClientActions::new(client.clone())),
        state: Arc::new(super::EphemeralStateStore::default()),
        registry: crate::providers::default_registry(),
        client,
        // No archive: these tests exercise the backend-facing members. The
        // history member's own behaviour without one is tested separately.
        archive: None,
    }
}

/// Stand a broker up, serve `service` on it, and return a proxy to it.
///
/// The serving connection comes back with the proxy and must be held: dropping
/// it releases the bus name, and every call then fails with `NameHasNoOwner`
/// rather than anything that points at the real cause.
async fn proxy_to(
    service: ConnectorService,
) -> tinybus::Result<(Connection, Connection, tinybus::Proxy, MemoryBus)> {
    let bus = MemoryBus::new();
    Broker::new().spawn(bus.clone());

    let serving = Connection::connect(bus.connect().await?).await?;
    serving
        .serve_at(names::OBJECT_PATH.try_into()?, service)
        .await?;
    serving.request_name(names::INTERFACE).await?;

    let client = Connection::connect(bus.connect().await?).await?;
    let proxy = client.proxy(names::INTERFACE, names::OBJECT_PATH, names::INTERFACE)?;
    Ok((serving, client, proxy, bus))
}

#[test]
fn declared_methods_match_the_dispatch_table() {
    let methods = service_over(StubTransport::replying(json!({})))
        .members()
        .into_iter()
        .map(|member| member.to_string())
        .collect::<Vec<_>>();

    assert_eq!(methods, names::METHODS.to_vec());
}

#[test]
fn the_served_interface_name_matches_the_contract() {
    assert_eq!(
        service_over(StubTransport::replying(json!({})))
            .name()
            .to_string(),
        names::INTERFACE
    );
}

#[test]
fn the_module_config_selects_the_proxy_route() {
    let config: ModuleConfig = serde_json::from_value(json!({
        "route": "proxy",
        "base_url": "https://api.example.com",
        "auth_token": "t0ken"
    }))
    .expect("parses");

    let route = config.into_route().expect("builds");
    assert_eq!(route.name(), "proxy");
}

#[test]
fn the_module_config_selects_the_direct_route() {
    let config: ModuleConfig = serde_json::from_value(json!({
        "route": "direct",
        "api_key": "sk-test"
    }))
    .expect("parses");

    // No base_url: production takes Composio's own API base.
    let route = config.into_route().expect("builds");
    assert_eq!(route.name(), "direct");
}

#[test]
fn the_module_config_requires_the_credential_its_route_needs() {
    // Failing at load beats producing a module that answers every member with
    // a 401 an hour later.
    for blob in [
        json!({ "route": "proxy", "base_url": "https://api.example.com" }),
        json!({ "route": "direct" }),
        json!({ "base_url": "https://api.example.com", "auth_token": "t" }),
    ] {
        assert!(
            serde_json::from_value::<ModuleConfig>(blob.clone()).is_err(),
            "{blob} must be refused"
        );
    }
}

#[test]
fn the_module_config_refuses_a_base_url_that_would_leak_the_credential() {
    let config: ModuleConfig = serde_json::from_value(json!({
        "route": "proxy",
        "base_url": "http://127.0.0.1:8080@evil.com",
        "auth_token": "t0ken"
    }))
    .expect("parses");

    let error = config.into_route().expect_err("must refuse");
    assert!(matches!(error, crate::Error::InsecureBaseUrl { .. }));
}

#[tokio::test]
async fn serves_the_toolkit_allowlist_over_a_real_bus() -> tinybus::Result<()> {
    let transport = StubTransport::replying(json!({ "toolkits": ["gmail", "notion"] }));
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport)).await?;

    let reply: ComposioToolkitsResponse = proxy.call(names::methods::LIST_TOOLKITS, ()).await?;
    assert_eq!(reply.toolkits, vec!["gmail", "notion"]);
    Ok(())
}

#[tokio::test]
async fn serves_connections_including_the_inactive_ones() -> tinybus::Result<()> {
    let transport = StubTransport::replying(json!({
        "connections": [
            { "id": "a", "toolkit": "gmail", "status": "ACTIVE" },
            { "id": "b", "toolkit": "instagram", "status": "PENDING" }
        ]
    }));
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport)).await?;

    let reply: ComposioConnectionsResponse =
        proxy.call(names::methods::LIST_CONNECTIONS, ()).await?;
    assert_eq!(reply.connections.len(), 2);
    assert!(!reply.connections[1].is_active());
    Ok(())
}

#[tokio::test]
async fn carries_authorize_arguments_across_the_bus() -> tinybus::Result<()> {
    let transport = StubTransport::replying(json!({
        "connectUrl": "https://composio.dev/oauth/abc",
        "connectionId": "conn_1"
    }));
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport.clone())).await?;

    let reply: ComposioAuthorizeResponse = proxy
        .call(
            names::methods::AUTHORIZE,
            (ComposioAuthorizeRequest {
                toolkit: "whatsapp".into(),
                extra_params: Some(json!({ "waba_id": "123" })),
            },),
        )
        .await?;

    assert_eq!(reply.connect_url, "https://composio.dev/oauth/abc");
    // The optional argument survived the round trip rather than being dropped
    // by the envelope — which would only show up as an upstream rejection.
    let body = transport.last_body.lock().unwrap().clone().unwrap();
    assert_eq!(body["waba_id"], "123");
    Ok(())
}

#[tokio::test]
async fn deletes_a_connection_over_the_bus() -> tinybus::Result<()> {
    let transport = StubTransport::replying(json!({
        "deleted": true, "memory_chunks_deleted": 3
    }));
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport)).await?;

    let reply: ComposioDeleteResponse = proxy
        .call(
            names::methods::DELETE_CONNECTION,
            (ComposioDeleteConnectionRequest {
                connection_id: "conn_9".into(),
                clear_memory: true,
            },),
        )
        .await?;

    assert!(reply.deleted);
    assert_eq!(reply.memory_chunks_deleted, 3);
    Ok(())
}

#[tokio::test]
async fn serves_the_tool_catalog_over_a_real_bus() -> tinybus::Result<()> {
    let transport = StubTransport::replying(json!({
        "tools": [{ "function": { "name": "GMAIL_SEND_EMAIL" } }]
    }));
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport)).await?;

    let reply: ComposioToolsResponse = proxy
        .call(
            names::methods::LIST_TOOLS,
            (ComposioListToolsRequest {
                toolkits: vec!["gmail".into()],
                tags: Vec::new(),
            },),
        )
        .await?;

    assert_eq!(reply.tools.len(), 1);
    assert_eq!(reply.tools[0].function.name, "GMAIL_SEND_EMAIL");
    Ok(())
}

#[tokio::test]
async fn runs_an_action_over_the_bus() -> tinybus::Result<()> {
    let transport = StubTransport::replying(json!({
        "data": { "messageId": "m-1" },
        "successful": true,
        "costUsd": 0.0025
    }));
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport.clone())).await?;

    let reply: ComposioExecuteResponse = proxy
        .call(
            names::methods::EXECUTE,
            (ComposioExecuteRequest {
                tool: "GMAIL_SEND_EMAIL".into(),
                arguments: Some(json!({ "to": "a@b.com" })),
                connection_id: Some("conn_1".into()),
            },),
        )
        .await?;

    assert!(reply.successful);
    assert_eq!(reply.data["messageId"], "m-1");

    // The connection the caller named must reach the backend: without it the
    // action runs against whichever account happens to be ambient.
    let body = transport.last_body.lock().unwrap().clone().unwrap();
    assert_eq!(body["connectionId"], "conn_1");
    assert_eq!(body["tool"], "GMAIL_SEND_EMAIL");
    Ok(())
}

#[tokio::test]
async fn a_provider_refusal_crosses_the_bus_as_a_reply_not_a_failure() -> tinybus::Result<()> {
    // The distinction matters: a caller that only checks for a member error
    // would report a failed send as a success.
    let transport = StubTransport::replying(json!({
        "successful": false,
        "error": "insufficient authentication scopes"
    }));
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport)).await?;

    let reply: ComposioExecuteResponse = proxy
        .call(
            names::methods::EXECUTE,
            (ComposioExecuteRequest {
                tool: "GMAIL_SEND_EMAIL".into(),
                arguments: Some(json!({ "to": "a@b.com" })),
                connection_id: None,
            },),
        )
        .await?;

    assert!(!reply.successful);
    let error = reply.error.expect("carries the failure");
    assert!(error.starts_with("[composio:error:insufficient_scope]"));
    Ok(())
}

#[tokio::test]
async fn local_argument_validation_fails_the_member_before_any_request() -> tinybus::Result<()> {
    let transport = StubTransport::replying(json!({}));
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport.clone())).await?;

    let result = proxy
        .call::<ComposioExecuteResponse>(
            names::methods::EXECUTE,
            (ComposioExecuteRequest {
                tool: "GMAIL_SEND_EMAIL".into(),
                arguments: Some(json!({ "subject": "hi" })),
                connection_id: None,
            },),
        )
        .await;

    let Err(error) = result else {
        return Err(tinybus::Error::failed(
            "a send with no recipient unexpectedly succeeded",
        ));
    };
    assert!(error.to_string().contains("recipient"));
    assert!(transport.last_body.lock().unwrap().is_none());
    Ok(())
}

#[tokio::test]
async fn reports_a_backend_failure_to_the_caller() -> tinybus::Result<()> {
    let transport = StubTransport::failing("502 bad gateway");
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport)).await?;

    let result = proxy
        .call::<ComposioToolkitsResponse>(names::methods::LIST_TOOLKITS, ())
        .await;

    let Err(error) = result else {
        return Err(tinybus::Error::failed(
            "a failing backend unexpectedly succeeded",
        ));
    };
    // The path and the upstream message both have to survive: without them a
    // host sees only "call failed".
    let rendered = error.to_string();
    assert!(rendered.contains("502 bad gateway"), "{rendered}");
    assert!(
        rendered.contains("/agent-integrations/composio/toolkits"),
        "{rendered}"
    );
    Ok(())
}

#[tokio::test]
async fn rejects_an_empty_toolkit_over_the_bus() -> tinybus::Result<()> {
    let transport = StubTransport::replying(json!({}));
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport)).await?;

    let result = proxy
        .call::<ComposioAuthorizeResponse>(
            names::methods::AUTHORIZE,
            (ComposioAuthorizeRequest {
                toolkit: "   ".into(),
                extra_params: None,
            },),
        )
        .await;

    let Err(error) = result else {
        return Err(tinybus::Error::failed(
            "an empty toolkit unexpectedly succeeded",
        ));
    };
    assert!(error.to_string().contains("toolkit must not be empty"));
    Ok(())
}
