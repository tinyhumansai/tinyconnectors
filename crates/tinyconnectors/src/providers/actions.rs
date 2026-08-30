//! Adapting the module's client to the provider action seam.

use std::sync::{Arc, PoisonError, RwLock};

use async_trait::async_trait;
use tinyconnectors_sync::{ActionRunner, Error as SyncError, Result as SyncResult};

use crate::client::ComposioClient;

/// Runs provider actions through this module's Composio client.
///
/// The client is optional because the module loads without one: a host that
/// only wants the capability members should not have to supply a credential.
/// A provider that then tries to run an action gets a named refusal rather than
/// a module that would not load at all.
///
/// It is *shared* rather than owned because the route can be replaced while the
/// module runs — a user signs in after a lazily-loaded module was already up.
/// A runner holding its own copy would keep running syncs against the
/// credential the module happened to start with, which after a sign-out is one
/// that answers 401 to everything.
#[derive(Debug, Clone)]
pub struct ClientActions {
    client: Arc<RwLock<Option<ComposioClient>>>,
}

impl ClientActions {
    /// Build a runner sharing the module's current client.
    #[must_use]
    pub fn new(client: Arc<RwLock<Option<ComposioClient>>>) -> Self {
        Self { client }
    }
}

/// What to report when a provider refuses an action.
///
/// The execute pipeline formats a message for every failed response, so the
/// fallback is not expected — but "the provider said no and would not say why"
/// is still more useful to whoever reads the sync log than an empty string.
pub(crate) fn refusal_message(error: Option<String>) -> String {
    error
        .map(|error| error.trim().to_string())
        .filter(|error| !error.is_empty())
        .unwrap_or_else(|| "the provider reported failure".to_string())
}

#[async_trait]
impl ActionRunner for ClientActions {
    async fn run(
        &self,
        action: &str,
        arguments: serde_json::Value,
        connection_id: &str,
    ) -> SyncResult<serde_json::Value> {
        let client = self
            .client
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or_else(|| SyncError::Action {
                action: action.to_string(),
                message: "this module was loaded without a connector route".to_string(),
            })?;

        let response = client
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
                message: refusal_message(response.error),
            });
        }

        Ok(response.data)
    }
}
