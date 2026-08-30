//! Minimal end-to-end usage of the crate, over a stub transport.
//!
//! Examples are compiled and linted in CI, so they cannot drift from the API.
//! This one uses a stub rather than a live backend so it runs anywhere and
//! needs no credential. Run it with:
//!
//! ```sh
//! cargo run -p tinyconnectors --example basic
//! ```

use std::sync::Arc;

use async_trait::async_trait;
use tinyconnectors::client::{ComposioClient, Transport};
use tinyconnectors::{Result, oauth};

/// A transport that answers from canned JSON, standing in for the backend.
#[derive(Debug)]
struct StubBackend;

#[async_trait]
impl Transport for StubBackend {
    async fn get(&self, path: &str) -> Result<serde_json::Value> {
        Ok(match path {
            "/agent-integrations/composio/toolkits" => {
                serde_json::json!({ "toolkits": ["gmail", "instagram"] })
            }
            _ => serde_json::json!({
                "connections": [
                    { "id": "c1", "toolkit": "gmail", "status": "ACTIVE" },
                    { "id": "c2", "toolkit": "instagram", "status": "PENDING" }
                ]
            }),
        })
    }

    async fn post(&self, _path: &str, _body: &serde_json::Value) -> Result<serde_json::Value> {
        Ok(serde_json::json!({
            "connectUrl": "https://composio.dev/oauth/abc",
            "connectionId": "c3"
        }))
    }

    async fn delete(&self, _path: &str) -> Result<serde_json::Value> {
        Ok(serde_json::json!({ "deleted": true, "memory_chunks_deleted": 0 }))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let client = ComposioClient::new(Arc::new(StubBackend));

    let toolkits = client.list_toolkits().await?;
    println!("connectable: {}", toolkits.toolkits.join(", "));

    // Non-active rows are what an abandoned OAuth handoff leaves behind. A
    // fresh handoff for a Meta toolkit clears them first, because Meta rate-
    // limits an account that accumulates them.
    for connection in client.list_connections().await?.connections {
        if connection.is_active() {
            println!("connected: {}", connection.toolkit);
        } else if oauth::is_meta_oauth_toolkit(&connection.toolkit)
            && oauth::is_clearable_oauth_status(&connection.status)
        {
            println!(
                "clearing stale {} handoff before retrying",
                connection.toolkit
            );
            client.delete_connection(&connection.id).await?;
        }
    }

    let handoff = client.authorize("instagram", None).await?;
    println!("open this to finish linking: {}", handoff.connect_url);

    Ok(())
}
