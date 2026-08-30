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

    fn answer(&self, path: &str) -> Result<serde_json::Value> {
        *self.calls.lock().unwrap() += 1;
        if let Some(message) = self.fail.lock().unwrap().clone() {
            return Err(Error::Transport {
                path: path.to_string(),
                message,
            });
        }
        Ok(self.reply.lock().unwrap().clone())
    }

    fn calls(&self) -> u32 {
        *self.calls.lock().unwrap()
    }
}

#[async_trait]
impl Transport for FakeTransport {
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
    let transport = FakeTransport::replying(json!({ "redirectUrl": "https://composio.dev/x" }));
    let direct = DirectRoute::new(transport.clone(), "sk", "   ");

    direct
        .authorize("gmail", &json!({ "toolkit": "gmail" }))
        .await
        .unwrap();

    let body = transport.last_body.lock().unwrap().clone().unwrap();
    assert_eq!(body["entity_id"], "default");
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
    // sockets; this module has no socket and no public endpoint. A direct-mode
    // subscription would be created and then deliver to nobody.
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
    assert!(resp.connections[0].toolkit.is_empty());
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
async fn authorize_reads_the_v3_redirect_url_and_stamps_the_entity() {
    let transport = FakeTransport::replying(json!({
        "redirectUrl": "https://composio.dev/oauth/xyz"
    }));
    let resp = route(transport.clone())
        .authorize("gmail", &json!({ "toolkit": "gmail" }))
        .await
        .unwrap();

    assert_eq!(resp.connect_url, "https://composio.dev/oauth/xyz");
    // v3's link response carries no connection id; an empty one is the
    // documented contract, not a decode failure.
    assert!(resp.connection_id.is_empty());

    let body = transport.last_body.lock().unwrap().clone().unwrap();
    assert_eq!(body["entity_id"], "entity-1");
}

#[tokio::test]
async fn authorize_reports_a_link_response_with_no_url() {
    let transport = FakeTransport::replying(json!({ "unexpected": true }));
    let error = route(transport)
        .authorize("gmail", &json!({ "toolkit": "gmail" }))
        .await
        .unwrap_err();

    assert!(matches!(error, Error::Decode { .. }));
    assert!(error.to_string().contains("redirect URL"));
}

#[tokio::test]
async fn gates_a_key_composio_keeps_rejecting() {
    let transport = FakeTransport::failing("401 invalid api key");
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
async fn a_success_clears_the_failure_count() {
    let transport = FakeTransport::failing("401 invalid api key");
    let direct = route(transport.clone());

    for _ in 0..2 {
        assert!(direct.list_connections().await.is_err());
    }

    // The user fixed the key.
    *transport.fail.lock().unwrap() = None;
    *transport.reply.lock().unwrap() = json!({ "items": [] });
    assert!(direct.list_connections().await.is_ok());

    // The old tally must not carry over and gate the working key.
    *transport.fail.lock().unwrap() = Some("401 invalid api key".to_string());
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
