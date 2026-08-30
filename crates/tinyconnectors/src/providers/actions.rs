//! Adapting the module's client to the provider action seam.

use async_trait::async_trait;
use tinyconnectors_sync::{ActionRunner, Error as SyncError, Result as SyncResult};

use crate::client::ComposioClient;

/// Runs provider actions through this module's Composio client.
#[derive(Debug, Clone)]
pub struct ClientActions {
    client: ComposioClient,
}

impl ClientActions {
    /// Build a runner over `client`.
    #[must_use]
    pub fn new(client: ComposioClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl ActionRunner for ClientActions {
    async fn run(
        &self,
        action: &str,
        arguments: serde_json::Value,
        connection_id: &str,
    ) -> SyncResult<serde_json::Value> {
        let response = self
            .client
            .execute(action, Some(arguments), Some(connection_id))
            .await
            .map_err(|error| SyncError::Action {
                action: action.to_string(),
                message: error.to_string(),
            })?;

        if !response.successful {
            // A refusal is an error for a provider even though `Execute`
            // reports it as a reply: a sync that treated a refused page as an
            // empty one would record "nothing new" and advance its cursor past
            // records it never read.
            return Err(SyncError::Action {
                action: action.to_string(),
                message: response
                    .error
                    .unwrap_or_else(|| "the provider reported failure".to_string()),
            });
        }

        Ok(response.data)
    }
}
