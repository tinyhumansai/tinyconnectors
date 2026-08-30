//! Unit tests for the provider action adapter.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;
use tinyconnectors_sync::{ActionRunner, Error as SyncError};

use super::ClientActions;
use crate::client::{ComposioClient, ProxyRoute, Transport};
use crate::Result;

#[derive(Debug, Default)]
struct StubTransport {
    reply: Mutex<serde_json::Value>,
    last_body: Mutex<Option<serde_json::Value>>,
}

#[async_trait]
impl Transport for StubTransport {
    async fn get(&self, _: &str) -> Result<serde_json::Value> {
        Ok(self.reply.lock().unwrap().clone())
    }
    async fn post(&self, _: &str, body: &serde_json::Value) -> Result<serde_json::Value> {
        *self.last_body.lock().unwrap() = Some(body.clone());
        Ok(self.reply.lock().unwrap().clone())
    }
    async fn delete(&self, _: &str) -> Result<serde_json::Value> {
        Ok(self.reply.lock().unwrap().clone())
    }
}

fn actions(reply: serde_json::Value) -> (Arc<StubTransport>, ClientActions) {
    let transport = Arc::new(StubTransport {
        reply: Mutex::new(reply),
        ..StubTransport::default()
    });
    let client = ComposioClient::new(Arc::new(ProxyRoute::new(transport.clone())));
    (transport, ClientActions::new(client))
}

#[tokio::test]
async fn returns_the_provider_payload_not_the_envelope() {
    // A provider wants the data. Handing it the envelope would make every
    // provider unwrap the same field.
    let (_transport, actions) = actions(json!({
        "successful": true,
        "data": { "messages": [{ "id": "m1" }] }
    }));

    let data = actions
        .run("GMAIL_FETCH_EMAILS", json!({}), "conn_1")
        .await
        .unwrap();
    assert_eq!(data["messages"][0]["id"], "m1");
}

#[tokio::test]
async fn targets_the_connection_it_was_given() {
    let (transport, actions) = actions(json!({ "successful": true, "data": {} }));
    actions
        .run("GMAIL_FETCH_EMAILS", json!({ "max": 10 }), "conn_7")
        .await
        .unwrap();

    let body = transport.last_body.lock().unwrap().clone().unwrap();
    assert_eq!(body["connectionId"], "conn_7");
    assert_eq!(body["arguments"]["max"], 10);
}

#[tokio::test]
async fn a_refused_action_is_an_error_here() {
    // `Execute` reports this as a successful reply. For a provider it must not
    // be: a sync that treated a refused page as an empty one would advance its
    // cursor past records it never read.
    let (_transport, actions) = actions(json!({
        "successful": false,
        "error": "insufficient authentication scopes"
    }));

    let error = actions
        .run("GMAIL_FETCH_EMAILS", json!({}), "conn_1")
        .await
        .unwrap_err();

    assert!(matches!(error, SyncError::Action { .. }));
    assert!(error.to_string().contains("GMAIL_FETCH_EMAILS"));
    assert!(error.to_string().contains("insufficient_scope"));
}

#[tokio::test]
async fn a_refusal_with_no_message_still_says_something() {
    let (_transport, actions) = actions(json!({ "successful": false }));
    let error = actions
        .run("GMAIL_FETCH_EMAILS", json!({}), "conn_1")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("reported failure"));
}
