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
    ComposioActiveTriggersResponse, ComposioAgentReadyToolkitsResponse, ComposioAuthorizeRequest,
    ComposioAuthorizeResponse, ComposioAvailableTriggersResponse, ComposioCapabilitiesResponse,
    ComposioConnectionsResponse, ComposioCreateTriggerRequest, ComposioCreateTriggerResponse,
    ComposioDeleteConnectionRequest, ComposioDeleteResponse, ComposioDisableTriggerRequest,
    ComposioDisableTriggerResponse, ComposioEnableTriggerRequest, ComposioEnableTriggerResponse,
    ComposioExecuteRequest, ComposioExecuteResponse, ComposioGetUserScopesRequest,
    ComposioGithubReposResponse, ComposioListAvailableTriggersRequest,
    ComposioListGithubReposRequest, ComposioListToolsRequest, ComposioListTriggerHistoryRequest,
    ComposioListTriggersRequest, ComposioRefreshIdentitiesResponse, ComposioSetUserScopesRequest,
    ComposioToolkitsResponse, ComposioToolsResponse, ComposioTriggerHistoryResult,
    ComposioUserProfile, ComposioUserProfileRequest, ComposioUserScopes,
    ComposioUserScopesResponse, ConnectorSyncRequest, ConnectorSyncResponse, SyncStage, names,
};

use super::{ConnectorService, ModuleConfig};
use crate::client::{ComposioClient, ProxyRoute, Transport};
use crate::{Error, Result};

#[derive(Debug, Default)]
struct StubTransport {
    reply: Mutex<serde_json::Value>,
    /// Replies keyed by a substring of the path, tried before `reply`.
    ///
    /// Several members call more than one endpoint — reading a profile lists
    /// connections and then executes an action — and answering both with one
    /// envelope tests the wrong thing.
    by_path: Mutex<Vec<(String, serde_json::Value)>>,
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

    /// Answer `value` for any path containing `needle`.
    fn answering(self: &Arc<Self>, needle: &str, value: serde_json::Value) -> Arc<Self> {
        self.by_path
            .lock()
            .unwrap()
            .push((needle.to_string(), value));
        self.clone()
    }

    fn answer(&self, path: &str) -> Result<serde_json::Value> {
        if let Some(message) = self.fail.lock().unwrap().clone() {
            return Err(Error::Transport {
                path: path.to_string(),
                message,
            });
        }
        for (needle, value) in self.by_path.lock().unwrap().iter() {
            if path.contains(needle.as_str()) {
                return Ok(value.clone());
            }
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
        actions: Arc::new(crate::providers::ClientActions::new(Some(client.clone()))),
        state: Arc::new(super::EphemeralStateStore::default()),
        registry: crate::providers::default_registry(),
        client: Some(client),
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
fn a_module_with_no_configuration_loads() {
    // A module that refuses to load without a credential cannot answer the
    // members that need none — and `ListCapabilities` is exactly what a
    // signed-out user deciding what to connect needs.
    let config: ModuleConfig = serde_json::from_value(json!({})).expect("parses");
    assert!(config.into_route().expect("builds").is_none());
}

#[tokio::test]
async fn an_unconfigured_module_answers_the_capability_members() -> tinybus::Result<()> {
    let (_serving, proxy) = serve_via_setup(ModuleConfig::default()).await?;

    let reply: ComposioCapabilitiesResponse =
        proxy.call(names::methods::LIST_CAPABILITIES, ()).await?;
    assert!(!reply.capabilities.is_empty());
    Ok(())
}

#[tokio::test]
async fn an_unconfigured_module_says_what_is_missing() -> tinybus::Result<()> {
    // Rather than an obscure failure, or a call that silently does nothing.
    let (_serving, proxy) = serve_via_setup(ModuleConfig::default()).await?;

    let result = proxy
        .call::<ComposioToolkitsResponse>(names::methods::LIST_TOOLKITS, ())
        .await;

    let Err(error) = result else {
        return Err(tinybus::Error::failed(
            "an unconfigured module listed toolkits",
        ));
    };
    let rendered = error.to_string();
    assert!(rendered.contains("route"), "{rendered}");
    assert!(rendered.contains("proxy"), "{rendered}");
    Ok(())
}

#[test]
fn the_module_config_selects_the_proxy_route() {
    let config: ModuleConfig = serde_json::from_value(json!({
        "route": "proxy",
        "base_url": "https://api.example.com",
        "auth_token": "t0ken"
    }))
    .expect("parses");

    let route = config.into_route().expect("builds").expect("has a route");
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
    let route = config.into_route().expect("builds").expect("has a route");
    assert_eq!(route.name(), "direct");
}

#[test]
fn the_module_config_requires_the_credential_its_route_needs() {
    // Failing at load beats producing a module that answers every member with
    // a 401 an hour later.
    for blob in [
        json!({ "route": "proxy", "base_url": "https://api.example.com" }),
        json!({ "route": "direct" }),
    ] {
        assert!(
            serde_json::from_value::<ModuleConfig>(blob.clone()).is_err(),
            "{blob} names a route without its credential and must be refused"
        );
    }

    // A blob with no `route` at all is not an error — it is an unconfigured
    // module, which is allowed.
    let config: ModuleConfig =
        serde_json::from_value(json!({ "base_url": "https://api.example.com" })).expect("parses");
    assert!(config.into_route().expect("builds").is_none());
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
                ..ComposioListToolsRequest::default()
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
async fn hides_an_action_the_scope_preference_forbids() -> tinybus::Result<()> {
    // A listing is what an agent picks from. Showing it an action it will then
    // be refused wastes a turn and reads to the model as a malfunction.
    let transport = StubTransport::replying(json!({
        "tools": [
            { "function": { "name": "GMAIL_FETCH_EMAILS" } },
            { "function": { "name": "GMAIL_SEND_EMAIL" } },
            { "function": { "name": "GMAIL_DELETE_MESSAGE" } }
        ]
    }));
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport)).await?;

    // The default preference: read and write, but not admin.
    let reply: ComposioToolsResponse = proxy
        .call(
            names::methods::LIST_TOOLS,
            (ComposioListToolsRequest::default(),),
        )
        .await?;

    let names_offered: Vec<_> = reply
        .tools
        .iter()
        .map(|tool| tool.function.name.as_str())
        .collect();
    assert_eq!(names_offered, ["GMAIL_FETCH_EMAILS", "GMAIL_SEND_EMAIL"]);
    Ok(())
}

#[tokio::test]
async fn shows_the_whole_catalog_when_the_caller_asks_for_it() -> tinybus::Result<()> {
    // A settings screen rendering the choices is not an agent about to act.
    let transport = StubTransport::replying(json!({
        "tools": [{ "function": { "name": "GMAIL_DELETE_MESSAGE" } }]
    }));
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport)).await?;

    let reply: ComposioToolsResponse = proxy
        .call(
            names::methods::LIST_TOOLS,
            (ComposioListToolsRequest {
                apply_user_scopes: false,
                ..ComposioListToolsRequest::default()
            },),
        )
        .await?;
    assert_eq!(reply.tools.len(), 1);
    Ok(())
}

#[tokio::test]
async fn refuses_to_run_an_action_the_scope_preference_forbids() -> tinybus::Result<()> {
    // Enforced by the module, not trusted from the caller: a preference a
    // caller could opt out of is a suggestion, not a restriction.
    let transport = StubTransport::replying(json!({ "successful": true }));
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport.clone())).await?;

    let result = proxy
        .call::<ComposioExecuteResponse>(
            names::methods::EXECUTE,
            (ComposioExecuteRequest {
                tool: "GMAIL_DELETE_MESSAGE".into(),
                arguments: None,
                connection_id: None,
            },),
        )
        .await;

    let Err(error) = result else {
        return Err(tinybus::Error::failed("a forbidden action ran"));
    };
    assert!(error.to_string().contains("not permitted"));
    assert!(
        transport.last_body.lock().unwrap().is_none(),
        "nothing may reach the backend"
    );
    Ok(())
}

#[tokio::test]
async fn a_scope_preference_round_trips_and_then_permits_the_action() -> tinybus::Result<()> {
    let transport = StubTransport::replying(json!({ "successful": true, "data": {} }));
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport)).await?;

    let stored: ComposioUserScopesResponse = proxy
        .call(
            names::methods::SET_USER_SCOPES,
            (ComposioSetUserScopesRequest {
                toolkit: "Gmail".into(),
                scopes: ComposioUserScopes {
                    read: true,
                    write: true,
                    admin: true,
                },
            },),
        )
        .await?;
    assert_eq!(stored.toolkit, "gmail", "the key is normalized");
    assert!(stored.scopes.admin);

    let read_back: ComposioUserScopesResponse = proxy
        .call(
            names::methods::GET_USER_SCOPES,
            (ComposioGetUserScopesRequest {
                toolkit: "gmail".into(),
            },),
        )
        .await?;
    assert!(read_back.scopes.admin);

    // And the action the default forbade now runs.
    let reply: ComposioExecuteResponse = proxy
        .call(
            names::methods::EXECUTE,
            (ComposioExecuteRequest {
                tool: "GMAIL_DELETE_MESSAGE".into(),
                arguments: None,
                connection_id: None,
            },),
        )
        .await?;
    assert!(reply.successful);
    Ok(())
}

#[tokio::test]
async fn a_toolkit_with_no_stored_preference_reports_the_default() -> tinybus::Result<()> {
    let transport = StubTransport::replying(json!({}));
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport)).await?;

    let reply: ComposioUserScopesResponse = proxy
        .call(
            names::methods::GET_USER_SCOPES,
            (ComposioGetUserScopesRequest {
                toolkit: "notion".into(),
            },),
        )
        .await?;

    assert!(reply.scopes.read);
    assert!(reply.scopes.write);
    assert!(
        !reply.scopes.admin,
        "admin is off until a user says otherwise"
    );
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
async fn syncs_a_toolkit_into_records_without_storing_them() -> tinybus::Result<()> {
    // The module reads a connected account and hands the records back. It
    // stores nothing: memory does that, over its own bus API.
    let transport = StubTransport::replying(json!({
        "successful": true,
        "data": { "data": { "messages": [
            { "id": "m1", "subject": "Hi", "snippet": "there" },
            { "id": "m2", "subject": "Again", "snippet": "hello" }
        ] } }
    }));
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport)).await?;

    let reply: ConnectorSyncResponse = proxy
        .call(
            names::methods::SYNC,
            (ConnectorSyncRequest {
                toolkit: "gmail".into(),
                connection_id: Some("conn_1".into()),
                source_id: Some("gmail:primary".into()),
                max_items: Some(10),
                reason: Some("scheduled".into()),
            },),
        )
        .await?;

    assert_eq!(reply.stage, SyncStage::Completed);
    assert_eq!(reply.batch.records.len(), 2);
    assert_eq!(reply.batch.toolkit, "gmail");
    assert_eq!(reply.batch.source_id, "gmail:primary");
    assert!(reply.batch.complete, "the provider had no next page");
    assert_eq!(reply.pages_read, 1);
    Ok(())
}

#[tokio::test]
async fn a_second_sync_skips_what_the_first_already_read() -> tinybus::Result<()> {
    // The cursor and the seen-set are the module's, so a caller does not carry
    // one — and a re-run does not re-ingest a user's whole mailbox.
    let transport = StubTransport::replying(json!({
        "successful": true,
        "data": { "data": { "messages": [{ "id": "m1", "subject": "Hi" }] } }
    }));
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport)).await?;

    let request = ConnectorSyncRequest {
        toolkit: "gmail".into(),
        connection_id: Some("conn_1".into()),
        ..ConnectorSyncRequest::default()
    };

    let first: ConnectorSyncResponse = proxy.call(names::methods::SYNC, (request.clone(),)).await?;
    assert_eq!(first.batch.records.len(), 1);

    let second: ConnectorSyncResponse = proxy.call(names::methods::SYNC, (request,)).await?;
    assert!(second.batch.records.is_empty(), "already ingested");
    assert_eq!(second.records_skipped, 1);
    Ok(())
}

#[tokio::test]
async fn syncing_a_toolkit_with_no_provider_says_so() -> tinybus::Result<()> {
    let transport = StubTransport::replying(json!({}));
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport)).await?;

    let result = proxy
        .call::<ConnectorSyncResponse>(
            names::methods::SYNC,
            (ConnectorSyncRequest {
                toolkit: "dropbox".into(),
                connection_id: Some("conn_1".into()),
                ..ConnectorSyncRequest::default()
            },),
        )
        .await;

    let Err(error) = result else {
        return Err(tinybus::Error::failed(
            "an unknown toolkit unexpectedly synced",
        ));
    };
    assert!(error.to_string().contains("no provider"));
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

// ── capability and identity members ──────────────────────────────────

#[tokio::test]
async fn reports_the_capability_matrix_without_touching_the_backend() -> tinybus::Result<()> {
    // It describes the compiled build, so it must answer with no session and
    // no request — that is what lets a UI tell "you cannot connect this" apart
    // from "you can connect it, but nothing will read it yet".
    let transport = StubTransport::replying(json!({}));
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport.clone())).await?;

    let reply: ComposioCapabilitiesResponse =
        proxy.call(names::methods::LIST_CAPABILITIES, ()).await?;

    assert!(!reply.capabilities.is_empty());
    assert!(reply.capabilities.iter().any(|row| row.toolkit == "gmail"));
    assert!(
        transport.last_body.lock().unwrap().is_none(),
        "no request may be made"
    );
    Ok(())
}

#[tokio::test]
async fn reports_which_toolkits_are_agent_ready() -> tinybus::Result<()> {
    let transport = StubTransport::replying(json!({}));
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport)).await?;

    let reply: ComposioAgentReadyToolkitsResponse = proxy
        .call(names::methods::LIST_AGENT_READY_TOOLKITS, ())
        .await?;

    assert!(reply.toolkits.contains(&"gmail".to_string()));
    // Sorted, so a UI list does not reshuffle between calls.
    let mut sorted = reply.toolkits.clone();
    sorted.sort();
    assert_eq!(reply.toolkits, sorted);
    Ok(())
}

#[tokio::test]
async fn reads_a_connected_account_s_identity() -> tinybus::Result<()> {
    let transport = StubTransport::replying(json!({
        "successful": true,
        "data": { "emailAddress": "user@example.com" }
    }));
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport)).await?;

    let reply: ComposioUserProfile = proxy
        .call(
            names::methods::GET_USER_PROFILE,
            (ComposioUserProfileRequest {
                toolkit: "gmail".into(),
                connection_id: Some("conn_1".into()),
            },),
        )
        .await?;

    assert_eq!(reply.toolkit, "gmail");
    assert_eq!(reply.email.as_deref(), Some("user@example.com"));
    Ok(())
}

#[tokio::test]
async fn a_profile_for_a_toolkit_with_no_provider_names_the_toolkit() -> tinybus::Result<()> {
    let transport = StubTransport::replying(json!({}));
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport)).await?;

    let result = proxy
        .call::<ComposioUserProfile>(
            names::methods::GET_USER_PROFILE,
            (ComposioUserProfileRequest {
                toolkit: "dropbox".into(),
                connection_id: Some("conn_1".into()),
            },),
        )
        .await;

    let Err(error) = result else {
        return Err(tinybus::Error::failed(
            "an unknown toolkit returned a profile",
        ));
    };
    assert!(error.to_string().contains("dropbox"));
    Ok(())
}

#[tokio::test]
async fn a_refresh_reports_the_broken_connections_beside_the_working_ones() -> tinybus::Result<()> {
    // A refresh exists to find the broken ones, so one of them must not hide
    // the rest by failing the whole call.
    let transport = StubTransport::replying(json!({
        "connections": [
            { "id": "c1", "toolkit": "gmail", "status": "ACTIVE" },
            { "id": "c2", "toolkit": "dropbox", "status": "ACTIVE" },
            { "id": "c3", "toolkit": "gmail", "status": "PENDING" }
        ]
    }))
    .answering(
        "/execute",
        json!({ "successful": true, "data": { "emailAddress": "a@b.com" } }),
    );
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport)).await?;

    let reply: ComposioRefreshIdentitiesResponse = proxy
        .call(names::methods::REFRESH_ALL_IDENTITIES, ())
        .await?;

    // gmail has a provider; dropbox does not; the pending row is skipped.
    assert_eq!(reply.profiles.len(), 1);
    assert_eq!(reply.failures.len(), 1);
    assert_eq!(reply.failures[0].toolkit, "dropbox");
    Ok(())
}

#[tokio::test]
async fn falls_back_to_the_first_active_connection_for_a_toolkit() -> tinybus::Result<()> {
    let transport = StubTransport::replying(json!({
        "connections": [
            { "id": "c1", "toolkit": "gmail", "status": "PENDING" },
            { "id": "c2", "toolkit": "gmail", "status": "ACTIVE" }
        ]
    }))
    .answering("/execute", json!({ "successful": true, "data": {} }));
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport)).await?;

    // No connection named: the pending row must not be chosen.
    let reply: ComposioUserProfile = proxy
        .call(
            names::methods::GET_USER_PROFILE,
            (ComposioUserProfileRequest {
                toolkit: "gmail".into(),
                connection_id: None,
            },),
        )
        .await?;

    assert_eq!(
        reply.connection_id.as_deref(),
        Some("c2"),
        "the active connection, not the pending one"
    );
    Ok(())
}

#[tokio::test]
async fn a_toolkit_with_no_active_connection_says_so() -> tinybus::Result<()> {
    let transport = StubTransport::replying(json!({ "connections": [] }));
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport)).await?;

    let result = proxy
        .call::<ComposioUserProfile>(
            names::methods::GET_USER_PROFILE,
            (ComposioUserProfileRequest {
                toolkit: "gmail".into(),
                connection_id: None,
            },),
        )
        .await;

    let Err(error) = result else {
        return Err(tinybus::Error::failed(
            "a profile came back with no connection",
        ));
    };
    assert!(error.to_string().contains("no active connection"));
    Ok(())
}

// ── trigger members ──────────────────────────────────────────────────

#[tokio::test]
async fn serves_the_github_repository_listing() -> tinybus::Result<()> {
    let transport = StubTransport::replying(json!({
        "connectionId": "c1",
        "repositories": [{ "owner": "a", "repo": "b", "fullName": "a/b" }]
    }));
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport)).await?;

    let reply: ComposioGithubReposResponse = proxy
        .call(
            names::methods::LIST_GITHUB_REPOS,
            (ComposioListGithubReposRequest {
                connection_id: Some("c1".into()),
            },),
        )
        .await?;
    assert_eq!(reply.repositories[0].full_name, "a/b");
    Ok(())
}

#[tokio::test]
async fn serves_the_trigger_catalog_and_the_active_list() -> tinybus::Result<()> {
    let transport = StubTransport::replying(json!({
        "triggers": [{
            "slug": "GMAIL_NEW_GMAIL_MESSAGE", "scope": "static",
            "id": "t1", "toolkit": "gmail", "connectionId": "c1"
        }]
    }));
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport)).await?;

    let available: ComposioAvailableTriggersResponse = proxy
        .call(
            names::methods::LIST_AVAILABLE_TRIGGERS,
            (ComposioListAvailableTriggersRequest {
                toolkit: "gmail".into(),
                connection_id: None,
            },),
        )
        .await?;
    assert_eq!(available.triggers.len(), 1);

    let active: ComposioActiveTriggersResponse = proxy
        .call(
            names::methods::LIST_TRIGGERS,
            (ComposioListTriggersRequest {
                toolkit: Some("gmail".into()),
            },),
        )
        .await?;
    assert_eq!(active.triggers[0].id, "t1");
    Ok(())
}

#[tokio::test]
async fn creates_enables_and_disables_a_trigger() -> tinybus::Result<()> {
    let transport = StubTransport::replying(json!({
        "triggerId": "t1", "slug": "GMAIL_NEW_GMAIL_MESSAGE",
        "connectionId": "c1", "deleted": true
    }));
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport)).await?;

    let created: ComposioCreateTriggerResponse = proxy
        .call(
            names::methods::CREATE_TRIGGER,
            (ComposioCreateTriggerRequest {
                slug: "GMAIL_NEW_GMAIL_MESSAGE".into(),
                connection_id: Some("c1".into()),
                trigger_config: Some(json!({ "labelIds": ["INBOX"] })),
            },),
        )
        .await?;
    assert_eq!(created.trigger_id, "t1");

    let enabled: ComposioEnableTriggerResponse = proxy
        .call(
            names::methods::ENABLE_TRIGGER,
            (ComposioEnableTriggerRequest {
                connection_id: "c1".into(),
                slug: "GMAIL_NEW_GMAIL_MESSAGE".into(),
                trigger_config: None,
            },),
        )
        .await?;
    assert_eq!(enabled.connection_id, "c1");

    let disabled: ComposioDisableTriggerResponse = proxy
        .call(
            names::methods::DISABLE_TRIGGER,
            (ComposioDisableTriggerRequest {
                trigger_id: "t1".into(),
            },),
        )
        .await?;
    assert!(disabled.deleted);
    Ok(())
}

#[tokio::test]
async fn trigger_history_without_a_state_dir_explains_itself() -> tinybus::Result<()> {
    // Rather than an empty list, which reads as "no trigger ever fired".
    let transport = StubTransport::replying(json!({}));
    let (_serving, _client, proxy, _bus) = proxy_to(service_over(transport)).await?;

    let result = proxy
        .call::<ComposioTriggerHistoryResult>(
            names::methods::LIST_TRIGGER_HISTORY,
            (ComposioListTriggerHistoryRequest { limit: Some(5) },),
        )
        .await;

    let Err(error) = result else {
        return Err(tinybus::Error::failed("history answered with no archive"));
    };
    assert!(error.to_string().contains("state_dir"));
    Ok(())
}

// ── config ───────────────────────────────────────────────────────────

#[test]
fn the_direct_route_accepts_a_loopback_base_url_for_testing() {
    let config: ModuleConfig = serde_json::from_value(json!({
        "route": "direct",
        "api_key": "sk-test",
        "entity_id": "e1",
        "base_url": "http://127.0.0.1:8080"
    }))
    .expect("parses");
    assert_eq!(
        config
            .into_route()
            .expect("builds")
            .expect("has a route")
            .name(),
        "direct"
    );
}

#[test]
fn a_state_dir_is_optional_on_both_routes() {
    for blob in [
        json!({ "route": "proxy", "base_url": "https://api.example.com", "auth_token": "t" }),
        json!({ "route": "direct", "api_key": "k" }),
    ] {
        let config: ModuleConfig = serde_json::from_value(blob.clone()).expect("parses");
        assert!(config.state_dir().is_none(), "{blob}");
    }
}

#[test]
fn a_named_state_dir_is_carried_through() {
    let config: ModuleConfig = serde_json::from_value(json!({
        "route": "proxy",
        "base_url": "https://api.example.com",
        "auth_token": "t",
        "state_dir": "/var/lib/openhuman"
    }))
    .expect("parses");
    assert_eq!(
        config.state_dir().map(std::path::Path::to_path_buf),
        Some(std::path::PathBuf::from("/var/lib/openhuman"))
    );
}

#[test]
fn an_unrecognized_sync_reason_is_treated_as_manual() {
    // The reason is a log line and a status label. Failing a sync over one
    // would break a working integration for a cosmetic field.
    use tinyconnectors_sync::SyncReason;
    assert_eq!(super::sync_reason(Some("scheduled")), SyncReason::Scheduled);
    assert_eq!(super::sync_reason(Some("trigger")), SyncReason::Trigger);
    assert_eq!(
        super::sync_reason(Some("initial_connect")),
        SyncReason::InitialConnect
    );
    assert_eq!(super::sync_reason(Some("nonsense")), SyncReason::Manual);
    assert_eq!(super::sync_reason(None), SyncReason::Manual);
}

#[tokio::test]
async fn the_ephemeral_state_store_round_trips() {
    // The fallback for a host that named no directory: sync state that lives
    // only as long as the module.
    use tinyconnectors_sync::SyncStateStore;
    let store = super::EphemeralStateStore::default();

    assert!(store.get("ns", "k").await.unwrap().is_none());
    store.set("ns", "k", &json!({ "a": 1 })).await.unwrap();
    assert_eq!(store.get("ns", "k").await.unwrap().unwrap()["a"], 1);
    assert!(store.get("other", "k").await.unwrap().is_none());
}

// ── setup ────────────────────────────────────────────────────────────

/// A scratch directory that removes itself.
struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "tinyconnectors-module-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("scratch directory");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Stand the module up the way the loader does: through `setup`.
async fn serve_via_setup(config: ModuleConfig) -> tinybus::Result<(Connection, tinybus::Proxy)> {
    let bus = MemoryBus::new();
    Broker::new().spawn(bus.clone());

    let serving = Connection::connect(bus.connect().await?).await?;
    super::setup(serving.clone(), config).await?;

    let client = Connection::connect(bus.connect().await?).await?;
    let proxy = client.proxy(names::INTERFACE, names::OBJECT_PATH, names::INTERFACE)?;
    // The client connection is returned so the caller holds it; the serving one
    // owns the bus name and must outlive the proxy.
    Ok((serving, proxy))
}

#[tokio::test]
async fn setup_serves_the_interface_over_the_bus() -> tinybus::Result<()> {
    let dir = TempDir::new("setup");
    let config: ModuleConfig = serde_json::from_value(json!({
        "route": "proxy",
        "base_url": "https://api.example.invalid",
        "auth_token": "t0ken",
        "state_dir": dir.0,
    }))
    .expect("parses");

    let (_serving, proxy) = serve_via_setup(config).await?;

    // A member that needs no network proves the whole path stood up: config
    // parsed, archive opened, route built, object served, name claimed.
    let reply: ComposioCapabilitiesResponse =
        proxy.call(names::methods::LIST_CAPABILITIES, ()).await?;
    assert!(!reply.capabilities.is_empty());
    Ok(())
}

#[tokio::test]
async fn setup_on_the_direct_route_serves_too() -> tinybus::Result<()> {
    let config: ModuleConfig = serde_json::from_value(json!({
        "route": "direct",
        "api_key": "sk-test",
    }))
    .expect("parses");

    let (_serving, proxy) = serve_via_setup(config).await?;
    let reply: ComposioAgentReadyToolkitsResponse = proxy
        .call(names::methods::LIST_AGENT_READY_TOOLKITS, ())
        .await?;
    assert!(!reply.toolkits.is_empty());
    Ok(())
}

#[tokio::test]
async fn setup_refuses_a_state_dir_it_cannot_use() -> tinybus::Result<()> {
    // Failing at load beats failing on the first trigger, weeks later.
    let config: ModuleConfig = serde_json::from_value(json!({
        "route": "proxy",
        "base_url": "https://api.example.invalid",
        "auth_token": "t0ken",
        "state_dir": "/proc/nonexistent-for-tests",
    }))
    .expect("parses");

    assert!(serve_via_setup(config).await.is_err());
    Ok(())
}

#[tokio::test]
async fn setup_refuses_a_base_url_that_would_leak_the_credential() -> tinybus::Result<()> {
    let config: ModuleConfig = serde_json::from_value(json!({
        "route": "proxy",
        "base_url": "http://127.0.0.1:8080@evil.com",
        "auth_token": "t0ken",
    }))
    .expect("parses");

    assert!(serve_via_setup(config).await.is_err());
    Ok(())
}

#[tokio::test]
async fn trigger_history_reads_the_archive_the_host_gave_it() -> tinybus::Result<()> {
    let dir = TempDir::new("history");
    let config: ModuleConfig = serde_json::from_value(json!({
        "route": "proxy",
        "base_url": "https://api.example.invalid",
        "auth_token": "t0ken",
        "state_dir": dir.0,
    }))
    .expect("parses");

    // A delivery recorded through the same archive the module opened.
    let archive = crate::triggers::TriggerArchive::open(&dir.0).expect("opens");
    archive
        .record(
            "gmail",
            "GMAIL_NEW_GMAIL_MESSAGE",
            "evt-1",
            "u",
            &json!({ "s": 1 }),
        )
        .expect("records");

    let (_serving, proxy) = serve_via_setup(config).await?;
    let reply: ComposioTriggerHistoryResult = proxy
        .call(
            names::methods::LIST_TRIGGER_HISTORY,
            (ComposioListTriggerHistoryRequest { limit: Some(10) },),
        )
        .await?;

    assert_eq!(reply.entries.len(), 1);
    assert_eq!(reply.entries[0].metadata_id, "evt-1");
    Ok(())
}

#[tokio::test]
async fn a_named_state_dir_persists_the_scope_preference() {
    // The difference between the two stores, observable: a preference written
    // through one module instance is read back by the next.
    let dir = TempDir::new("persist");
    let store = super::state_store(Some(&dir.0));
    tinyconnectors_sync::UserScopePref {
        read: true,
        write: false,
        admin: false,
    }
    .save(store.as_ref(), "gmail")
    .await
    .expect("saves");

    let reopened = super::state_store(Some(&dir.0));
    let pref = tinyconnectors_sync::UserScopePref::load(reopened.as_ref(), "gmail")
        .await
        .expect("loads");
    assert!(!pref.write, "the choice survived the module restart");
}

#[tokio::test]
async fn an_unnamed_state_dir_keeps_nothing_between_instances() {
    // Documented, not accidental: a host that means to sync should name one.
    let store = super::state_store(None);
    tinyconnectors_sync::UserScopePref {
        read: true,
        write: false,
        admin: false,
    }
    .save(store.as_ref(), "gmail")
    .await
    .expect("saves");

    let fresh = super::state_store(None);
    let pref = tinyconnectors_sync::UserScopePref::load(fresh.as_ref(), "gmail")
        .await
        .expect("loads");
    assert!(pref.write, "a new instance starts from the default");
}
