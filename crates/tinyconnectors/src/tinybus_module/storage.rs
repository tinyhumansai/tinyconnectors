//! The module's connection to the host's storage, over the storage bus.
//!
//! A cdylib has its own copy of every static in this crate, so a registry the
//! host fills in its own process is invisible here. The module instead calls the
//! host's `Storage` objects through its `TinyBus` connection
//! (`tinystoragedrivers_busproxy::connect_storage`), the same way every other
//! storage-using module does. The host binds the scope from the authenticated
//! caller and enforces the grant in [`crate::state::storage_needs`]; nothing
//! here names a scope, a directory or a backend.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tinybus::Connection;
use tinyconnectors_sync::PrefsStore;
use tinystoragedrivers_core::{
    DocumentStore, Result as StorageResult, ScopedStorage, StorageBackend, StorageError,
    StreamStore,
};
use tokio::sync::Mutex;

use crate::state::DocumentPrefsStore;

/// A lazily connected, scoped handle on the host's storage.
///
/// A connect failure is not cached, so a host that starts serving storage after
/// the module loaded (modules load lazily, often before sign-in) is picked up by
/// the next call.
#[derive(Clone)]
pub(super) struct StorageLink {
    connection: Option<Connection>,
    slot: Arc<Mutex<Option<ScopedStorage>>>,
}

impl std::fmt::Debug for StorageLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StorageLink")
            .field("connected", &self.connection.is_some())
            .finish_non_exhaustive()
    }
}

impl StorageLink {
    /// Reach the host's storage over `connection`.
    pub(super) fn new(connection: Connection) -> Self {
        Self {
            connection: Some(connection),
            slot: Arc::default(),
        }
    }

    /// A link with no host behind it: every call reports storage unavailable.
    #[cfg(test)]
    pub(super) fn none() -> Self {
        Self {
            connection: None,
            slot: Arc::default(),
        }
    }

    async fn scoped(&self) -> StorageResult<ScopedStorage> {
        let Some(connection) = self.connection.as_ref() else {
            return Err(StorageError::unavailable("no storage bus connection"));
        };
        let mut slot = self.slot.lock().await;
        if let Some(scoped) = slot.as_ref() {
            return Ok(scoped.clone());
        }
        tracing::debug!("[connectors][storage] connecting to the host storage bus");
        let bus = tinystoragedrivers_busproxy::connect_storage(connection.clone()).await?;
        let scoped = bus.for_scope(&tinystoragedrivers_busproxy::module_scope_label())?;
        *slot = Some(scoped.clone());
        Ok(scoped)
    }

    /// The scoped document store (scope preferences).
    pub(super) async fn documents(&self) -> StorageResult<Arc<dyn DocumentStore>> {
        Ok(Arc::clone(self.scoped().await?.documents()))
    }

    /// The scoped stream store (trigger archive).
    pub(super) async fn streams(&self) -> StorageResult<Arc<dyn StreamStore>> {
        Ok(Arc::clone(self.scoped().await?.streams()))
    }
}

/// Scope preferences on the host's documents, or in memory while the host
/// serves no storage.
///
/// The in-memory fallback keeps the members that need no persistence working
/// (and keeps a module loaded before the host's storage is up usable), at the
/// cost of forgetting the choice at restart; each fall-back is logged.
#[derive(Debug)]
pub(super) struct LinkedPrefsStore {
    link: StorageLink,
    fallback: super::EphemeralPrefsStore,
}

impl LinkedPrefsStore {
    pub(super) fn new(link: StorageLink) -> Self {
        Self {
            link,
            fallback: super::EphemeralPrefsStore::default(),
        }
    }
}

#[async_trait]
impl PrefsStore for LinkedPrefsStore {
    async fn get(&self, key: &str) -> tinyconnectors_sync::Result<Option<Value>> {
        match self.link.documents().await {
            Ok(documents) => DocumentPrefsStore::new(documents).get(key).await,
            Err(error) => {
                tracing::warn!(%error, "[connectors][prefs] host storage unavailable; reading in-memory preferences");
                self.fallback.get(key).await
            }
        }
    }

    async fn set(&self, key: &str, value: &Value) -> tinyconnectors_sync::Result<()> {
        match self.link.documents().await {
            Ok(documents) => DocumentPrefsStore::new(documents).set(key, value).await,
            Err(error) => {
                tracing::warn!(%error, "[connectors][prefs] host storage unavailable; keeping preference in memory only");
                self.fallback.set(key, value).await
            }
        }
    }
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;
