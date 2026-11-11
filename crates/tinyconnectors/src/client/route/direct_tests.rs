//! Unit tests for the direct route.
//!
//! Two things are worth testing here and neither is the happy path: the v3
//! envelope translation, which is where a shape change silently empties a
//! user's connection list, and the API-key gate, which is what stops a revoked
//! key from hammering Composio forever.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;

use super::{DirectRoute, INVALID_API_KEY_THRESHOLD, Route};
use crate::client::Transport;
use crate::{Error, Result};

#[derive(Debug, Default)]
struct FakeTransport {
    reply: Mutex<serde_json::Value>,
    fail: Mutex<Option<String>>,
    calls: Mutex<u32>,
    last_body: Mutex<Option<serde_json::Value>>,
    last_path: Mutex<Option<String>>,
    /// Reply to `GET /connected_accounts/{id}`, when set; `Err` fails the lookup.
    account: Mutex<Option<std::result::Result<serde_json::Value, String>>>,
    last_execute_body: Mutex<Option<serde_json::Value>>,
    /// Replies for paths starting with a prefix, checked before `reply`.
    routed: Mutex<Vec<(String, serde_json::Value)>>,
    /// Every POST, in order, as `(path, body)`.
    posts: Mutex<Vec<(String, serde_json::Value)>>,
}

impl FakeTransport {
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

    /// A route whose toolkit lookup finds one enabled, Composio-managed
    /// gmail auth config, `ac_gmail`, and whose other calls answer `link`.
    fn linking(link: serde_json::Value) -> Arc<Self> {
        let transport = Self::replying(link);
        transport.route_prefix(
            "/auth_configs?",
            json!({ "items": [gmail_config("ac_gmail", true, "ENABLED")] }),
        );
        transport
    }

    fn route_prefix(&self, prefix: &str, reply: serde_json::Value) {
        self.routed
            .lock()
            .unwrap()
            .push((prefix.to_string(), reply));
    }

    fn posts(&self) -> Vec<(String, serde_json::Value)> {
        self.posts.lock().unwrap().clone()
    }

    fn answer(&self, path: &str) -> Result<serde_json::Value> {
        *self.calls.lock().unwrap() += 1;
        *self.last_path.lock().unwrap() = Some(path.to_string());
        if let Some(message) = self.fail.lock().unwrap().clone() {
            return Err(Error::Transport {
                path: path.to_string(),
                message,
            });
        }
        if let Some((_, reply)) = self
            .routed
            .lock()
            .unwrap()
            .iter()
            .find(|(prefix, _)| path.starts_with(prefix.as_str()))
        {
            return Ok(reply.clone());
        }
        Ok(self.reply.lock().unwrap().clone())
    }

    fn last_path(&self) -> String {
        self.last_path.lock().unwrap().clone().unwrap()
    }

    fn calls(&self) -> u32 {
        *self.calls.lock().unwrap()
    }
}

#[async_trait]
impl Transport for FakeTransport {
    async fn get(&self, path: &str) -> Result<serde_json::Value> {
        if path.starts_with("/connected_accounts/")
            && let Some(account) = self.account.lock().unwrap().clone()
        {
            return account.map_err(|message| Error::Transport {
                path: path.to_string(),
                message,
            });
        }
        self.answer(path)
    }

    async fn post(&self, path: &str, body: &serde_json::Value) -> Result<serde_json::Value> {
        *self.last_body.lock().unwrap() = Some(body.clone());
        self.posts
            .lock()
            .unwrap()
            .push((path.to_string(), body.clone()));
        // Composio v3 validates the link body: it needs `auth_config_id` and
        // `user_id`, and answers the proxy-route shape with this 400.
        if path == "/connected_accounts/link"
            && (body.get("auth_config_id").is_none()
                || body.get("user_id").is_none()
                || body.get("toolkit").is_some())
        {
            return Err(Error::Transport {
                path: path.to_string(),
                message: "HTTP 400: Validation error while processing request".to_string(),
            });
        }
        if path.starts_with("/tools/execute/") {
            *self.last_execute_body.lock().unwrap() = Some(body.clone());
        }
        self.answer(path)
    }

    async fn delete(&self, path: &str) -> Result<serde_json::Value> {
        self.answer(path)
    }
}

/// A `GET /auth_configs` item for gmail.
fn gmail_config(id: &str, managed: bool, status: &str) -> serde_json::Value {
    json!({
        "id": id,
        "toolkit": { "slug": "gmail" },
        "is_composio_managed": managed,
        "status": status,
    })
}

fn route(transport: Arc<FakeTransport>) -> DirectRoute {
    DirectRoute::new(transport, "sk-test", "entity-1")
}

#[test]
fn names_itself_direct() {
    assert_eq!(route(FakeTransport::replying(json!({}))).name(), "direct");
}

#[tokio::test]
async fn falls_back_to_the_default_entity() {
    // An empty entity id must not travel to Composio as an empty string, which
    // it rejects; "default" is what the direct client has always sent.
    let transport = FakeTransport::linking(json!({ "redirectUrl": "https://composio.dev/x" }));
    let direct = DirectRoute::new(transport.clone(), "sk", "   ");

    direct
        .authorize("gmail", &json!({ "toolkit": "gmail" }))
        .await
        .unwrap();

    let body = transport.last_body.lock().unwrap().clone().unwrap();
    assert_eq!(body["user_id"], "default");
}

#[tokio::test]
async fn refuses_to_list_toolkits_and_says_why() {
    let transport = FakeTransport::replying(json!({}));
    let error = route(transport.clone()).list_toolkits().await.unwrap_err();

    assert!(matches!(
        error,
        Error::UnsupportedByRoute {
            route: "direct",
            member: "ListToolkits"
        }
    ));
    // A refusal must not have cost a request.
    assert_eq!(transport.calls(), 0);
}

#[tokio::test]
async fn refuses_to_delete_a_connection_and_says_why() {
    let transport = FakeTransport::replying(json!({}));
    let error = route(transport.clone())
        .delete_connection("c1")
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        Error::UnsupportedByRoute {
            route: "direct",
            member: "DeleteConnection"
        }
    ));
    assert_eq!(transport.calls(), 0);
}

#[tokio::test]
async fn refuses_every_trigger_member_and_says_why() {
    // A trigger is a webhook, and a webhook has to arrive somewhere. The proxy
    // backend HMAC-verifies deliveries and fans them out over the user's
    // sockets; this module has no socket and no public endpoint. Unless the
    // host says it receives deliveries itself, a direct-mode subscription
    // would be created and then deliver to nobody.
    let transport = FakeTransport::replying(json!({}));
    let direct = route(transport.clone());

    assert!(matches!(
        direct
            .list_available_triggers("gmail", None)
            .await
            .unwrap_err(),
        Error::UnsupportedByRoute {
            route: "direct",
            ..
        }
    ));
    assert!(direct.list_triggers(None).await.is_err());
    assert!(direct.create_trigger("slug", None, None).await.is_err());
    assert!(direct.enable_trigger("c", "s", None).await.is_err());
    assert!(direct.disable_trigger("t").await.is_err());
    assert!(direct.list_github_repos(None).await.is_err());

    assert_eq!(
        transport.calls(),
        0,
        "a refusal must not cost a request to Composio"
    );
}

#[tokio::test]
async fn translates_v3_connected_accounts_into_connections() {
    let transport = FakeTransport::replying(json!({
        "items": [{
            "id": "ca_1",
            "toolkit": { "slug": "gmail", "logo": "https://…" },
            "status": "ACTIVE",
            "createdAt": "2026-02-01T00:00:00Z",
            "email": "user@example.com"
        }]
    }));
    let resp = route(transport).list_connections().await.unwrap();

    assert_eq!(resp.connections.len(), 1);
    let connection = &resp.connections[0];
    assert_eq!(connection.id, "ca_1");
    // The object-wrapped toolkit is unwrapped, not dropped.
    assert_eq!(connection.toolkit, "gmail");
    assert!(connection.is_active());
    assert_eq!(
        connection.account_email.as_deref(),
        Some("user@example.com")
    );
}

#[tokio::test]
async fn accepts_a_bare_array_as_well_as_an_items_envelope() {
    let transport = FakeTransport::replying(json!([
        { "id": "ca_1", "toolkit": "slack", "status": "ACTIVE" }
    ]));
    let resp = route(transport).list_connections().await.unwrap();
    assert_eq!(resp.connections.len(), 1);
    assert_eq!(resp.connections[0].toolkit, "slack");
}

#[tokio::test]
async fn keeps_a_malformed_row_as_inactive_rather_than_dropping_it() {
    // Fail-safe direction: a row with no status must not vanish (which reads as
    // "disconnected") — it must show up as present but not usable.
    let transport = FakeTransport::replying(json!({
        "items": [{ "id": "ca_1" }]
    }));
    let resp = route(transport).list_connections().await.unwrap();

    assert_eq!(resp.connections.len(), 1);
    assert_eq!(resp.connections[0].toolkit.len(), 0);
    assert!(!resp.connections[0].is_active());
}

#[tokio::test]
async fn drops_only_a_row_with_no_id_at_all() {
    // Without an id there is nothing to disconnect or dedupe by, so the row is
    // not actionable in any way.
    let transport = FakeTransport::replying(json!({
        "items": [{ "toolkit": "gmail", "status": "ACTIVE" }]
    }));
    let resp = route(transport).list_connections().await.unwrap();
    assert!(resp.connections.is_empty());
}

#[tokio::test]
async fn authorize_links_the_toolkits_auth_config_to_the_entity() {
    let transport = FakeTransport::linking(json!({
        "redirectUrl": "https://composio.dev/oauth/xyz"
    }));
    let resp = route(transport.clone())
        .authorize(
            "gmail",
            &json!({ "toolkit": "gmail", "oauth_scopes": ["gmail.readonly"] }),
        )
        .await
        .unwrap();

    assert_eq!(resp.connect_url, "https://composio.dev/oauth/xyz");
    // v3's link response carries no connection id; an empty one is the
    // documented contract, not a decode failure.
    assert_eq!(resp.connection_id.len(), 0);

    // The lookup names the toolkit; the link sends exactly the v3 fields.
    assert!(
        transport
            .last_path()
            .starts_with("/connected_accounts/link"),
        "{}",
        transport.last_path()
    );
    let posts = transport.posts();
    assert_eq!(posts.len(), 1, "an existing config is reused, none created");
    assert_eq!(
        posts[0],
        (
            "/connected_accounts/link".to_string(),
            json!({ "auth_config_id": "ac_gmail", "user_id": "entity-1" })
        )
    );
}

#[tokio::test]
async fn authorize_creates_a_managed_auth_config_when_none_is_usable() {
    let transport = FakeTransport::linking(json!({ "redirect_url": "https://composio.dev/o" }));
    // Only a disabled gmail config and another toolkit's config exist.
    transport.routed.lock().unwrap().clear();
    transport.route_prefix(
        "/auth_configs?",
        json!({ "items": [
            gmail_config("ac_off", true, "DISABLED"),
            { "id": "ac_slack", "toolkit": { "slug": "slack" }, "status": "ENABLED" }
        ] }),
    );
    transport.route_prefix(
        "/auth_configs",
        json!({ "toolkit": { "slug": "gmail" }, "auth_config": { "id": "ac_new" } }),
    );

    route(transport.clone())
        .authorize("gmail", &json!({ "toolkit": "gmail" }))
        .await
        .unwrap();

    let posts = transport.posts();
    assert_eq!(
        posts[0],
        (
            "/auth_configs".to_string(),
            json!({
                "toolkit": { "slug": "gmail" },
                "auth_config": { "type": "use_composio_managed_auth" }
            })
        )
    );
    assert_eq!(posts[1].1["auth_config_id"], "ac_new");
}

#[tokio::test]
async fn authorize_prefers_a_composio_managed_auth_config() {
    let transport = FakeTransport::linking(json!({ "redirectUrl": "https://composio.dev/x" }));
    transport.routed.lock().unwrap().clear();
    transport.route_prefix(
        "/auth_configs?",
        json!({ "items": [
            gmail_config("ac_custom", false, "ENABLED"),
            gmail_config("ac_managed", true, "ENABLED")
        ] }),
    );

    route(transport.clone())
        .authorize("gmail", &json!({ "toolkit": "gmail" }))
        .await
        .unwrap();

    assert_eq!(transport.posts()[0].1["auth_config_id"], "ac_managed");
}

#[tokio::test]
async fn authorize_reports_a_created_auth_config_with_no_id() {
    let transport = FakeTransport::replying(json!({ "items": [] }));
    transport.route_prefix("/auth_configs", json!({ "auth_config": {} }));

    let error = route(transport)
        .authorize("gmail", &json!({ "toolkit": "gmail" }))
        .await
        .unwrap_err();

    assert!(matches!(error, Error::Decode { .. }), "{error}");
}

#[tokio::test]
async fn a_refused_connect_request_is_an_authorize_error_with_composios_detail() {
    let transport = FakeTransport::failing("HTTP 400: Validation error while processing request");

    let error = route(transport)
        .authorize("gmail", &json!({ "toolkit": "gmail" }))
        .await
        .unwrap_err();

    match error {
        Error::Authorize { toolkit, message } => {
            assert_eq!(toolkit, "gmail");
            assert!(message.contains("Validation error"), "{message}");
        }
        other => panic!("expected Error::Authorize, got {other:?}"),
    }
}

#[tokio::test]
async fn the_proxy_shaped_link_body_is_what_v3_rejects() {
    // Guards the fake itself: the old `{toolkit, entity_id}` body must fail,
    // so the tests above prove the new body rather than a permissive mock.
    let transport = FakeTransport::replying(json!({}));
    let error = transport
        .post(
            "/connected_accounts/link",
            &json!({ "toolkit": "gmail", "entity_id": "entity-1" }),
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("HTTP 400"), "{error}");
}

#[tokio::test]
async fn authorize_reports_a_link_response_with_no_url() {
    let transport = FakeTransport::linking(json!({ "unexpected": true }));
    let error = route(transport)
        .authorize("gmail", &json!({ "toolkit": "gmail" }))
        .await
        .unwrap_err();

    assert!(matches!(error, Error::Decode { .. }));
    assert!(error.to_string().contains("redirect URL"));
}

#[tokio::test]
async fn gates_a_key_composio_keeps_rejecting() {
    let transport = FakeTransport::failing("HTTP 401: Invalid API key");
    let direct = route(transport.clone());

    // Every attempt up to the threshold reaches Composio; the next is refused
    // locally without a request.
    for _ in 0..INVALID_API_KEY_THRESHOLD {
        assert!(direct.list_connections().await.is_err());
    }
    assert_eq!(transport.calls(), INVALID_API_KEY_THRESHOLD);

    let error = direct.list_connections().await.unwrap_err();
    assert!(matches!(error, Error::DirectAuthGated { .. }));
    assert!(error.to_string().contains("valid key"));
    assert_eq!(
        transport.calls(),
        INVALID_API_KEY_THRESHOLD,
        "the gate must stop the request, not just relabel its failure"
    );
}

#[tokio::test]
async fn does_not_gate_on_failures_that_say_nothing_about_the_key() {
    // A 500 or a dropped connection is not evidence the key is bad. Counting it
    // would gate a user whose key is fine because Composio had a bad afternoon.
    let transport = FakeTransport::failing("502 bad gateway");
    let direct = route(transport.clone());

    for _ in 0..5 {
        assert!(direct.list_connections().await.is_err());
    }
    assert_eq!(transport.calls(), 5, "every attempt must still be made");
}

#[tokio::test]
async fn does_not_gate_on_a_tool_error_that_mentions_an_api_key() {
    // The provider's own text is not the key verdict: only a 401 from
    // Composio is. A downstream tool's 400 must not lock the user out.
    let transport = FakeTransport::failing("HTTP 400: Invalid API key for Stripe");
    let direct = route(transport.clone());

    for _ in 0..INVALID_API_KEY_THRESHOLD + 2 {
        assert!(direct.list_connections().await.is_err());
    }
    assert_eq!(
        transport.calls(),
        INVALID_API_KEY_THRESHOLD + 2,
        "every attempt must still be made"
    );
}

#[tokio::test]
async fn a_success_clears_the_failure_count() {
    let transport = FakeTransport::failing("HTTP 401: Invalid API key");
    let direct = route(transport.clone());

    for _ in 0..2 {
        assert!(direct.list_connections().await.is_err());
    }

    // The user fixed the key.
    *transport.fail.lock().unwrap() = None;
    *transport.reply.lock().unwrap() = json!({ "items": [] });
    assert!(direct.list_connections().await.is_ok());

    // The old tally must not carry over and gate the working key.
    *transport.fail.lock().unwrap() = Some("HTTP 401: Invalid API key".to_string());
    for _ in 0..INVALID_API_KEY_THRESHOLD {
        assert!(direct.list_connections().await.is_err());
    }
    let before_gate = transport.calls();
    assert!(matches!(
        direct.list_connections().await.unwrap_err(),
        Error::DirectAuthGated { .. }
    ));
    assert_eq!(transport.calls(), before_gate);
}

#[tokio::test]
async fn lists_v3_tools_into_the_function_calling_envelope() {
    // v3 rows are the function itself, not the `{type, function}` envelope a
    // model expects, so the route wraps them.
    let transport = FakeTransport::replying(json!({
        "items": [{
            "slug": "GMAIL_SEND_EMAIL",
            "description": "Send an email",
            "input_parameters": { "type": "object" },
            "output_parameters": { "type": "object" }
        }]
    }));
    let tools = route(transport)
        .list_tools(&["gmail".to_string()], &[])
        .await
        .unwrap();

    assert_eq!(tools.tools.len(), 1);
    assert_eq!(tools.tools[0].kind, "function");
    assert_eq!(tools.tools[0].function.name, "GMAIL_SEND_EMAIL");
    assert!(tools.tools[0].function.parameters.is_some());
    assert!(tools.tools[0].function.output_parameters.is_some());
}

#[tokio::test]
async fn drops_a_tool_row_with_no_name() {
    let transport = FakeTransport::replying(json!({ "items": [{ "description": "no slug" }] }));
    let tools = route(transport).list_tools(&[], &[]).await.unwrap();
    assert!(tools.tools.is_empty());
}

#[tokio::test]
async fn executes_and_reshapes_a_v3_result() {
    let transport = FakeTransport::replying(json!({
        "successful": true,
        "data": { "messageId": "m-1" }
    }));
    let response = route(transport.clone())
        .execute("GMAIL_SEND_EMAIL", &json!({ "to": "a@b.com" }), Some("c1"))
        .await
        .unwrap();

    assert!(response.successful);
    assert_eq!(response.data["messageId"], "m-1");
    // Direct mode carries no billing margin: the user pays Composio, not us.
    assert!((response.cost_usd - 0.0).abs() < f64::EPSILON);
    // Composio renders no compact markdown; callers fall back to `data`.
    assert!(response.markdown_formatted.is_none());

    let body = transport.last_body.lock().unwrap().clone().unwrap();
    assert_eq!(body["connected_account_id"], "c1");
    assert_eq!(body["entity_id"], "entity-1");
}

#[tokio::test]
async fn treats_a_v3_result_with_no_verdict_as_successful() {
    // v3 answers a plain result for some actions. Defaulting to failure would
    // report a completed action as broken.
    let transport = FakeTransport::replying(json!({ "messageId": "m-1" }));
    let response = route(transport)
        .execute("GMAIL_SEND_EMAIL", &json!({}), None)
        .await
        .unwrap();

    assert!(response.successful);
    assert_eq!(response.data["messageId"], "m-1");
}

#[tokio::test]
async fn carries_a_v3_failure_through() {
    let transport = FakeTransport::replying(json!({
        "successful": false,
        "error": "insufficient scope"
    }));
    let response = route(transport)
        .execute("GMAIL_SEND_EMAIL", &json!({}), None)
        .await
        .unwrap();

    assert!(!response.successful);
    assert_eq!(response.error.as_deref(), Some("insufficient scope"));
}

#[tokio::test]
async fn treats_a_v3_result_with_an_error_and_no_verdict_as_failed() {
    let transport = FakeTransport::replying(json!({ "error": "insufficient scope" }));
    let response = route(transport)
        .execute("GMAIL_SEND_EMAIL", &json!({}), None)
        .await
        .unwrap();

    assert!(!response.successful);
    assert_eq!(response.error.as_deref(), Some("insufficient scope"));
}

#[tokio::test]
async fn authorize_keeps_a_connection_id_when_v3_supplies_one() {
    let transport = FakeTransport::linking(json!({
        "redirect_url": "https://composio.dev/oauth/z",
        "connectedAccountId": "ca_7"
    }));
    let response = route(transport)
        .authorize("gmail", &json!({ "toolkit": "gmail" }))
        .await
        .unwrap();

    assert_eq!(response.connection_id, "ca_7");
}

#[tokio::test]
async fn a_successful_call_clears_the_gate_for_later_calls() {
    let transport = FakeTransport::replying(json!({ "items": [] }));
    let direct = route(transport.clone());

    assert!(direct.list_connections().await.is_ok());
    assert!(direct.list_connections().await.is_ok());
    assert_eq!(transport.calls(), 2, "no gate ever closed");
}

#[tokio::test]
async fn connected_accounts_ask_for_a_generous_page() {
    let transport = FakeTransport::replying(json!({ "items": [] }));
    route(transport.clone()).list_connections().await.unwrap();
    assert_eq!(transport.last_path(), "/connected_accounts?limit=200");
}

#[tokio::test]
async fn tool_listing_pins_latest_toolkit_versions_and_repeats_tags() {
    let transport = FakeTransport::replying(json!({ "items": [] }));
    route(transport.clone())
        .list_tools(
            &["gmail".to_string(), " github ".to_string()],
            &["stars".to_string(), " ".to_string(), "repos".to_string()],
        )
        .await
        .unwrap();
    assert_eq!(
        transport.last_path(),
        "/tools?limit=200&toolkit_versions=latest&toolkits=gmail,github&tags=stars&tags=repos"
    );
}

#[tokio::test]
async fn tool_listing_without_filters_still_pins_versions() {
    let transport = FakeTransport::replying(json!({ "items": [] }));
    route(transport.clone()).list_tools(&[], &[]).await.unwrap();
    assert_eq!(
        transport.last_path(),
        "/tools?limit=200&toolkit_versions=latest"
    );
}

#[tokio::test]
async fn drops_a_blank_id_and_trims_a_padded_one() {
    let transport = FakeTransport::replying(json!({
        "items": [
            { "id": "   ", "toolkit": "gmail", "status": "ACTIVE" },
            { "id": "", "toolkit": "gmail", "status": "ACTIVE" },
            { "id": "  ca_2 ", "toolkit": "slack", "status": "ACTIVE" }
        ]
    }));
    let resp = route(transport).list_connections().await.unwrap();
    assert_eq!(resp.connections.len(), 1);
    assert_eq!(resp.connections[0].id, "ca_2");
}

#[tokio::test]
async fn a_blank_toolkit_falls_back_to_the_app_name() {
    let transport = FakeTransport::replying(json!({
        "items": [
            { "id": "a", "toolkit": " ", "app_name": " github ", "status": "ACTIVE" },
            { "id": "b", "toolkit": { "slug": "", "name": "notion" }, "status": "ACTIVE" }
        ]
    }));
    let resp = route(transport).list_connections().await.unwrap();
    assert_eq!(resp.connections[0].toolkit, "github");
    assert_eq!(resp.connections[1].toolkit, "notion");
}

#[tokio::test]
async fn a_tool_without_a_description_is_described_by_its_name() {
    let transport = FakeTransport::replying(json!({
        "items": [
            { "slug": "A_TOOL", "name": "A tool" },
            { "slug": "", "description": "blank slug" }
        ]
    }));
    let tools = route(transport).list_tools(&[], &[]).await.unwrap();
    assert_eq!(tools.tools.len(), 1);
    assert_eq!(
        tools.tools[0].function.description.as_deref(),
        Some("A tool")
    );
}

fn execute_body(transport: &FakeTransport) -> serde_json::Value {
    transport.last_execute_body.lock().unwrap().clone().unwrap()
}

#[tokio::test]
async fn execute_acts_as_the_connected_accounts_own_user() {
    let transport = FakeTransport::replying(json!({ "successful": true, "data": {} }));
    *transport.account.lock().unwrap() =
        Some(Ok(json!({ "id": "c1", "user_id": "pg-generated-user" })));
    route(transport.clone())
        .execute("GMAIL_SEND_EMAIL", &json!({}), Some("c1"))
        .await
        .unwrap();
    assert_eq!(execute_body(&transport)["entity_id"], "pg-generated-user");
}

#[tokio::test]
async fn execute_keeps_the_configured_entity_when_the_owner_is_unknown() {
    let transport = FakeTransport::replying(json!({ "successful": true, "data": {} }));
    *transport.account.lock().unwrap() = Some(Err("boom".to_string()));
    route(transport.clone())
        .execute("GMAIL_SEND_EMAIL", &json!({}), Some("c1"))
        .await
        .unwrap();
    assert_eq!(execute_body(&transport)["entity_id"], "entity-1");

    *transport.account.lock().unwrap() = Some(Ok(json!({ "id": "c1", "user_id": "  " })));
    route(transport.clone())
        .execute("GMAIL_SEND_EMAIL", &json!({}), Some("c1"))
        .await
        .unwrap();
    assert_eq!(execute_body(&transport)["entity_id"], "entity-1");
}

#[tokio::test]
async fn a_rejected_key_in_the_owner_lookup_counts_toward_the_gate() {
    let transport = FakeTransport::failing("HTTP 401: Invalid API key");
    let direct = route(transport.clone());

    // The lookup is the one request per execute: its rejection is returned,
    // not swallowed into a second doomed execute.
    for _ in 0..INVALID_API_KEY_THRESHOLD {
        assert!(
            direct
                .execute("GMAIL_SEND_EMAIL", &json!({}), Some("c1"))
                .await
                .is_err()
        );
    }
    assert_eq!(transport.calls(), INVALID_API_KEY_THRESHOLD);

    let error = direct
        .execute("GMAIL_SEND_EMAIL", &json!({}), Some("c1"))
        .await
        .unwrap_err();
    assert!(matches!(error, Error::DirectAuthGated { .. }));
    assert_eq!(transport.calls(), INVALID_API_KEY_THRESHOLD);
}
