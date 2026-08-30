//! Unit tests for the client, over a fixture transport.
//!
//! No network: [`FakeTransport`] answers from a fixture map and records what it
//! was asked, so a test can assert on the path and the request body — which is
//! where the interesting behavior is. The paths are a wire contract with the
//! backend just as much as the payloads are.
//!
//! These run the client over [`ProxyRoute`], because that is the route whose
//! paths and validation they are about. The direct route's own translation and
//! key-gating are tested beside it, in `route/direct_test.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;

use super::{ComposioClient, ProxyRoute, Transport};
use crate::{Error, Result};

/// One recorded request.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Call {
    verb: &'static str,
    path: String,
    body: Option<serde_json::Value>,
}

#[derive(Debug, Default)]
struct FakeTransport {
    reply: Mutex<serde_json::Value>,
    /// Replies keyed by a substring of the path, tried before `reply`.
    ///
    /// The OAuth pre-clean makes three different calls in one `authorize`, and
    /// answering all of them with one envelope tests the wrong thing.
    by_path: Mutex<Vec<(String, serde_json::Value)>>,
    fail: Mutex<Option<String>>,
    calls: Mutex<Vec<Call>>,
}

impl FakeTransport {
    /// Answer `value` for any path containing `needle`.
    ///
    /// The OAuth pre-clean makes three different calls inside one `authorize`,
    /// and answering all of them with one envelope tests the wrong thing.
    fn answering(self: &Arc<Self>, needle: &str, value: serde_json::Value) -> Arc<Self> {
        self.by_path
            .lock()
            .unwrap()
            .push((needle.to_string(), value));
        self.clone()
    }

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

    fn record(
        &self,
        verb: &'static str,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value> {
        self.calls.lock().unwrap().push(Call {
            verb,
            path: path.to_string(),
            body: body.cloned(),
        });
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

    fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl Transport for FakeTransport {
    async fn get(&self, path: &str) -> Result<serde_json::Value> {
        self.record("GET", path, None)
    }

    async fn post(&self, path: &str, body: &serde_json::Value) -> Result<serde_json::Value> {
        self.record("POST", path, Some(body))
    }

    async fn delete(&self, path: &str) -> Result<serde_json::Value> {
        self.record("DELETE", path, None)
    }
}

#[tokio::test]
async fn lists_toolkits_from_the_allowlist_path() {
    let transport = FakeTransport::replying(json!({ "toolkits": ["gmail", "notion"] }));
    let client = ComposioClient::new(Arc::new(ProxyRoute::new(transport.clone())));

    let resp = client.list_toolkits().await.unwrap();
    assert_eq!(resp.toolkits, vec!["gmail", "notion"]);
    assert_eq!(transport.calls()[0].verb, "GET");
    assert_eq!(
        transport.calls()[0].path,
        "/agent-integrations/composio/toolkits"
    );
}

#[tokio::test]
async fn lists_connections_without_filtering_out_inactive_rows() {
    // The OAuth cleanup path needs the non-active rows; filtering here would
    // hide exactly the debris it exists to clear.
    let transport = FakeTransport::replying(json!({
        "connections": [
            { "id": "a", "toolkit": "gmail", "status": "ACTIVE" },
            { "id": "b", "toolkit": "instagram", "status": "PENDING" }
        ]
    }));
    let client = ComposioClient::new(Arc::new(ProxyRoute::new(transport)));

    let resp = client.list_connections().await.unwrap();
    assert_eq!(resp.connections.len(), 2);
    assert!(!resp.connections[1].is_active());
}

#[tokio::test]
async fn authorize_posts_the_trimmed_toolkit() {
    let transport = FakeTransport::replying(json!({
        "connectUrl": "https://composio.dev/oauth/abc",
        "connectionId": "conn_1"
    }));
    let client = ComposioClient::new(Arc::new(ProxyRoute::new(transport.clone())));

    let resp = client.authorize("  notion  ", None).await.unwrap();
    assert_eq!(resp.connection_id, "conn_1");

    let call = &transport.calls()[0];
    assert_eq!(call.verb, "POST");
    assert_eq!(call.path, "/agent-integrations/composio/authorize");
    assert_eq!(call.body.as_ref().unwrap()["toolkit"], "notion");
}

#[tokio::test]
async fn authorize_merges_extra_params_into_the_body() {
    let transport = FakeTransport::replying(json!({
        "connectUrl": "u", "connectionId": "c"
    }));
    let client = ComposioClient::new(Arc::new(ProxyRoute::new(transport.clone())));

    client
        .authorize("whatsapp", Some(json!({ "waba_id": "123" })))
        .await
        .unwrap();

    let body = transport.calls()[0].body.clone().unwrap();
    assert_eq!(body["toolkit"], "whatsapp");
    assert_eq!(body["waba_id"], "123");
}

#[tokio::test]
async fn authorize_adds_the_gmail_read_scope_composio_omits() {
    let transport = FakeTransport::replying(json!({
        "connectUrl": "u", "connectionId": "c"
    }));
    let client = ComposioClient::new(Arc::new(ProxyRoute::new(transport.clone())));

    client.authorize("Gmail", None).await.unwrap();

    let body = transport.calls()[0].body.clone().unwrap();
    assert_eq!(
        body["oauth_scopes"],
        json!(["https://www.googleapis.com/auth/gmail.readonly"])
    );
}

#[tokio::test]
async fn authorize_keeps_caller_scopes_and_does_not_duplicate_required_ones() {
    let transport = FakeTransport::replying(json!({
        "connectUrl": "u", "connectionId": "c"
    }));
    let client = ComposioClient::new(Arc::new(ProxyRoute::new(transport.clone())));

    client
        .authorize(
            "gmail",
            Some(json!({
                "oauth_scopes": [
                    "https://www.googleapis.com/auth/gmail.send",
                    "https://www.googleapis.com/auth/gmail.readonly"
                ]
            })),
        )
        .await
        .unwrap();

    let body = transport.calls()[0].body.clone().unwrap();
    assert_eq!(
        body["oauth_scopes"],
        json!([
            "https://www.googleapis.com/auth/gmail.send",
            "https://www.googleapis.com/auth/gmail.readonly"
        ]),
        "a caller's scopes are kept and the required one is not appended twice"
    );
}

#[tokio::test]
async fn authorize_leaves_a_toolkit_with_no_extra_scopes_alone() {
    let transport = FakeTransport::replying(json!({
        "connectUrl": "u", "connectionId": "c"
    }));
    let client = ComposioClient::new(Arc::new(ProxyRoute::new(transport.clone())));

    client.authorize("notion", None).await.unwrap();

    let body = transport.calls()[0].body.clone().unwrap();
    assert!(body.get("oauth_scopes").is_none());
}

#[tokio::test]
async fn clears_abandoned_meta_handoffs_before_authorizing() {
    // Every authorize creates a connection row immediately, in a non-active
    // state. Instagram and Facebook share an OAuth host that 429s once too many
    // exist — so a user who clicked Connect twice is rate-limited out of the
    // retry that would have worked. Clearing first is the mitigation.
    let transport = FakeTransport::replying(json!({
        "connections": [
            { "id": "stale-1", "toolkit": "instagram", "status": "PENDING" },
            { "id": "stale-2", "toolkit": "instagram", "status": "FAILED" },
            { "id": "live", "toolkit": "instagram", "status": "ACTIVE" },
            { "id": "other", "toolkit": "gmail", "status": "PENDING" }
        ]
    }))
    .answering(
        "/authorize",
        json!({ "connectUrl": "u", "connectionId": "c" }),
    )
    .answering("/connections/", json!({ "deleted": true }));
    let client = ComposioClient::new(Arc::new(ProxyRoute::new(transport.clone())));

    client.authorize("instagram", None).await.unwrap();

    let deleted: Vec<_> = transport
        .calls()
        .into_iter()
        .filter(|call| call.verb == "DELETE")
        .map(|call| call.path)
        .collect();

    assert_eq!(deleted.len(), 2, "only the abandoned rows: {deleted:?}");
    assert!(deleted.iter().any(|path| path.ends_with("stale-1")));
    assert!(deleted.iter().any(|path| path.ends_with("stale-2")));
    assert!(
        !deleted.iter().any(|path| path.ends_with("live")),
        "an active connection must never be cleared"
    );
    assert!(
        !deleted.iter().any(|path| path.ends_with("other")),
        "another toolkit's rows are not ours to clear"
    );
}

#[tokio::test]
async fn does_not_clear_anything_for_a_toolkit_that_is_not_rate_limited() {
    // The cleanup exists for Meta's shared OAuth host. Deleting rows for every
    // toolkit would destroy in-flight handoffs for no benefit.
    let transport = FakeTransport::replying(json!({
        "connectUrl": "u", "connectionId": "c"
    }));
    let client = ComposioClient::new(Arc::new(ProxyRoute::new(transport.clone())));

    client.authorize("gmail", None).await.unwrap();

    assert!(
        !transport.calls().iter().any(|call| call.verb == "DELETE"),
        "no cleanup for a non-Meta toolkit"
    );
    assert_eq!(transport.calls().len(), 1, "just the authorize");
}

#[tokio::test]
async fn a_failed_cleanup_does_not_stop_the_handoff() {
    // The user asked to connect an account. Refusing to start because the
    // *cleanup* failed turns a possible rate limit into a certain failure.
    let transport = FakeTransport::failing("502 bad gateway");
    let client = ComposioClient::new(Arc::new(ProxyRoute::new(transport.clone())));

    // The authorize itself still fails here — but on its own error, having
    // tried, rather than being abandoned during cleanup.
    let error = client.authorize("instagram", None).await.unwrap_err();
    assert!(error.to_string().contains("502 bad gateway"));
    assert!(
        transport
            .calls()
            .iter()
            .any(|call| call.path.contains("/authorize")),
        "the handoff must still have been attempted"
    );
}

#[tokio::test]
async fn authorize_rejects_an_empty_toolkit_without_calling_out() {
    let transport = FakeTransport::replying(json!({}));
    let client = ComposioClient::new(Arc::new(ProxyRoute::new(transport.clone())));

    let error = client.authorize("   ", None).await.unwrap_err();
    assert!(matches!(error, Error::Authorize { .. }));
    assert!(transport.calls().is_empty(), "must not reach the backend");
}

#[tokio::test]
async fn authorize_refuses_extra_params_that_are_not_an_object() {
    let transport = FakeTransport::replying(json!({}));
    let client = ComposioClient::new(Arc::new(ProxyRoute::new(transport.clone())));

    let error = client
        .authorize("gmail", Some(json!(["not", "an", "object"])))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("must be a JSON object"));
    assert!(transport.calls().is_empty());
}

#[tokio::test]
async fn authorize_refuses_to_let_extra_params_override_a_reserved_key() {
    // Without this, a tool argument could redirect the handoff at a different
    // toolkit or credential than the one the caller asked for.
    for key in ["toolkit", "toolkit_version", "auth", "client_id"] {
        let transport = FakeTransport::replying(json!({}));
        let client = ComposioClient::new(Arc::new(ProxyRoute::new(transport.clone())));

        let error = client
            .authorize("gmail", Some(json!({ key: "hijacked" })))
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains(key),
            "{key} must be refused by name"
        );
        assert!(transport.calls().is_empty());
    }
}

#[tokio::test]
async fn deletes_a_connection_by_id() {
    let transport = FakeTransport::replying(json!({
        "deleted": true, "memory_chunks_deleted": 12
    }));
    let client = ComposioClient::new(Arc::new(ProxyRoute::new(transport.clone())));

    let resp = client.delete_connection(" conn_9 ").await.unwrap();
    assert!(resp.deleted);
    assert_eq!(resp.memory_chunks_deleted, 12);

    let call = &transport.calls()[0];
    assert_eq!(call.verb, "DELETE");
    assert_eq!(call.path, "/agent-integrations/composio/connections/conn_9");
}

#[tokio::test]
async fn delete_rejects_an_empty_connection_id_without_calling_out() {
    let transport = FakeTransport::replying(json!({}));
    let client = ComposioClient::new(Arc::new(ProxyRoute::new(transport.clone())));

    let error = client.delete_connection("  ").await.unwrap_err();
    assert!(matches!(error, Error::Authorize { .. }));
    assert!(
        transport.calls().is_empty(),
        "an empty id must not become a DELETE on the collection"
    );
}

#[tokio::test]
async fn lists_available_triggers_with_both_filters_encoded() {
    let transport = FakeTransport::replying(json!({ "triggers": [] }));
    let client = ComposioClient::new(Arc::new(ProxyRoute::new(transport.clone())));

    client
        .list_available_triggers("gmail", Some("conn_1"))
        .await
        .unwrap();

    let path = &transport.calls()[0].path;
    assert!(path.contains("toolkit=gmail"), "{path}");
    // Underscores are unreserved and must survive: an encoded id is a
    // different string from the one the backend stored.
    assert!(path.contains("connectionId=conn_1"), "{path}");
}

#[tokio::test]
async fn omits_an_absent_trigger_filter_rather_than_sending_an_empty_one() {
    // An empty parameter reads as "match nothing" on several endpoints, which
    // is the opposite of the "no filter" a caller meant.
    let transport = FakeTransport::replying(json!({ "triggers": [] }));
    let client = ComposioClient::new(Arc::new(ProxyRoute::new(transport.clone())));

    client.list_triggers(None).await.unwrap();
    assert_eq!(
        transport.calls()[0].path,
        "/agent-integrations/composio/triggers"
    );

    client.list_triggers(Some("   ")).await.unwrap();
    assert_eq!(
        transport.calls()[1].path,
        "/agent-integrations/composio/triggers",
        "a blank filter is absent, not a filter on the empty string"
    );
}

#[tokio::test]
async fn enables_a_trigger_with_its_configuration() {
    let transport = FakeTransport::replying(json!({
        "triggerId": "ti_1", "slug": "GMAIL_NEW_GMAIL_MESSAGE", "connectionId": "conn_1"
    }));
    let client = ComposioClient::new(Arc::new(ProxyRoute::new(transport.clone())));

    let resp = client
        .enable_trigger(
            "conn_1",
            "GMAIL_NEW_GMAIL_MESSAGE",
            Some(json!({ "labelIds": ["INBOX"] })),
        )
        .await
        .unwrap();
    assert_eq!(resp.trigger_id, "ti_1");

    let body = transport.calls()[0].body.clone().unwrap();
    assert_eq!(body["connectionId"], "conn_1");
    assert_eq!(body["triggerConfig"]["labelIds"][0], "INBOX");
}

#[tokio::test]
async fn refuses_an_empty_identifier_before_it_reaches_a_url() {
    // An empty id in a path addresses the collection instead of the item — a
    // DELETE on every trigger rather than on one.
    let transport = FakeTransport::replying(json!({}));
    let client = ComposioClient::new(Arc::new(ProxyRoute::new(transport.clone())));

    assert!(client.disable_trigger("  ").await.is_err());
    assert!(client.enable_trigger("", "slug", None).await.is_err());
    assert!(client.enable_trigger("conn_1", " ", None).await.is_err());
    assert!(client.create_trigger("", None, None).await.is_err());
    assert!(client.list_available_triggers("", None).await.is_err());
    assert!(
        transport.calls().is_empty(),
        "nothing may reach the backend"
    );
}

#[tokio::test]
async fn disables_a_trigger_by_id() {
    let transport = FakeTransport::replying(json!({ "deleted": true }));
    let client = ComposioClient::new(Arc::new(ProxyRoute::new(transport.clone())));

    assert!(client.disable_trigger("ti_1").await.unwrap().deleted);
    let call = &transport.calls()[0];
    assert_eq!(call.verb, "DELETE");
    assert_eq!(call.path, "/agent-integrations/composio/triggers/ti_1");
}

#[tokio::test]
async fn reports_a_transport_failure_unchanged() {
    let transport = FakeTransport::failing("502 bad gateway");
    let client = ComposioClient::new(Arc::new(ProxyRoute::new(transport)));

    let error = client.list_toolkits().await.unwrap_err();
    assert!(matches!(error, Error::Transport { .. }));
    assert!(error.to_string().contains("502 bad gateway"));
}

#[tokio::test]
async fn reports_an_unexpected_envelope_as_a_decode_failure() {
    let transport = FakeTransport::replying(json!({ "connections": "not-an-array" }));
    let client = ComposioClient::new(Arc::new(ProxyRoute::new(transport)));

    let error = client.list_connections().await.unwrap_err();
    assert!(matches!(error, Error::Decode { .. }));
    assert!(
        error
            .to_string()
            .contains("/agent-integrations/composio/connections"),
        "the failing path belongs in the message: {error}"
    );
}
