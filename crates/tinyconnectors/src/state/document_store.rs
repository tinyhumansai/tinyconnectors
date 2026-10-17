//! A preference store backed by one document per toolkit.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tinyconnectors_sync::{Error as SyncError, PREFS_NAMESPACE, PrefsStore, Result as SyncResult};
use tinystoragedrivers_core::{DocumentStore, Key, Precondition, StorageError, WriteOptions};

use tinyconnectors_bus::storage::{PREFS_SCHEMA_VERSION, decode_preference, encode_preference};

/// Scope preferences persisted through a [`DocumentStore`] port.
///
/// The collection is [`PREFS_NAMESPACE`] and the document id is
/// `Key::of([toolkit])`. The body is the preference itself: an object is stored
/// as it is, and any other value under a reserved field (see
/// [`storage_schema`](super::storage_schema)). The port is already bound to the caller's scope, so one user's
/// preferences are never visible to another.
#[derive(Debug, Clone)]
pub struct DocumentPrefsStore {
    documents: Arc<dyn DocumentStore>,
}

impl DocumentPrefsStore {
    /// Store preferences in `documents`.
    #[must_use]
    pub fn new(documents: Arc<dyn DocumentStore>) -> Self {
        Self { documents }
    }
}

fn failed(key: &str, action: &str, error: &StorageError) -> SyncError {
    tracing::debug!(key, action, %error, "[connectors][prefs] storage call failed");
    SyncError::Store {
        key: key.to_string(),
        message: format!("could not {action}: {error}"),
    }
}

/// The document id for a toolkit key.
fn document_id(key: &str) -> SyncResult<Key> {
    Key::of([key]).map_err(|error| failed(key, "address the preference", &error))
}

#[async_trait]
impl PrefsStore for DocumentPrefsStore {
    async fn get(&self, key: &str) -> SyncResult<Option<Value>> {
        let id = document_id(key)?;
        let found = self
            .documents
            .get(PREFS_NAMESPACE, id.as_str())
            .await
            .map_err(|error| failed(key, "read the preference", &error))?;
        // A toolkit the user never configured is the normal case, not a failure.
        Ok(found.map(|versioned| decode_preference(versioned.doc)))
    }

    async fn set(&self, key: &str, value: &Value) -> SyncResult<()> {
        let id = document_id(key)?;
        self.documents
            .put_with(
                PREFS_NAMESPACE,
                id.as_str(),
                encode_preference(value),
                Precondition::None,
                WriteOptions::new().schema(PREFS_SCHEMA_VERSION),
            )
            .await
            .map(|_| ())
            .map_err(|error| failed(key, "write the preference", &error))
    }
}
