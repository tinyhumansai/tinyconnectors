//! The host-supplied key-value seam the scope preferences are kept in.

use async_trait::async_trait;

use crate::Result;

/// Where per-toolkit preferences are persisted, supplied by the host.
///
/// Two methods over JSON, on purpose: this crate must not depend on a storage
/// engine, and a seam any wider would start describing one. A host backs it
/// with whatever it already has. Keys are already normalized by
/// [`UserScopePref::key`](super::UserScopePref::key); an implementation keeps
/// them under [`PREFS_NAMESPACE`](super::PREFS_NAMESPACE) if it has namespaces.
#[async_trait]
pub trait PrefsStore: Send + Sync {
    /// Read a value, or `None` if the key has never been written.
    ///
    /// # Errors
    ///
    /// Returns an error only when the store itself failed. A missing key is
    /// `Ok(None)`: a toolkit the user has not configured is the normal case,
    /// not a failure.
    async fn get(&self, key: &str) -> Result<Option<serde_json::Value>>;

    /// Write a value, replacing any previous one.
    ///
    /// # Errors
    ///
    /// Returns an error when the store failed to persist the value.
    async fn set(&self, key: &str, value: &serde_json::Value) -> Result<()>;
}
