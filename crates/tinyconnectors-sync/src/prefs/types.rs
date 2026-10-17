//! The per-toolkit scope preference and its storage.

use super::store::PrefsStore;
use crate::{Error, Result};

// Declared once, in the contract crate beside the storage schema that uses them.
pub use tinyconnectors_bus::storage::{LEGACY_PREFS_NAMESPACE, PREFS_NAMESPACE};

pub use tinyconnectors_bus::vocabulary::UserScopePref;

/// Provider-side persistence for the shared scope preference payload.
///
/// Import this trait to use `UserScopePref::load` and `preference.save`.
/// The contract type itself holds no store or runtime dependency.
pub trait ScopePrefStore: Sized {
    /// Load the preference for a toolkit, returning defaults only for a missing row.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Store`] on store failure or [`Error::Decode`] for a malformed row.
    fn load(
        store: &dyn PrefsStore,
        toolkit: &str,
    ) -> impl std::future::Future<Output = Result<Self>> + Send;

    /// Persist this preference for a toolkit.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Store`] when the store fails to write the row.
    fn save(
        self,
        store: &dyn PrefsStore,
        toolkit: &str,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
}

impl ScopePrefStore for UserScopePref {
    /// The preference stored for `toolkit`, or the default if none is.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Store`] when the store failed, or [`Error::Decode`]
    /// when the stored row is not this shape. Neither falls back to the
    /// default: a preference that cannot be read must not quietly become
    /// permission the user did not grant.
    async fn load(store: &dyn PrefsStore, toolkit: &str) -> Result<Self> {
        let key = Self::key(toolkit);
        let Some(value) = store.get(&key).await? else {
            return Ok(Self::default());
        };
        serde_json::from_value(value).map_err(|error| Error::Decode {
            key,
            message: error.to_string(),
        })
    }

    /// Persist this preference for `toolkit`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Store`] when the store failed to write it. Building the
    /// value cannot fail — three booleans always serialize — so it is built
    /// directly rather than through a fallible conversion with a branch nothing
    /// can reach.
    async fn save(self, store: &dyn PrefsStore, toolkit: &str) -> Result<()> {
        let value = serde_json::json!({
            "read": self.read,
            "write": self.write,
            "admin": self.admin,
        });
        store.set(&Self::key(toolkit), &value).await
    }
}
