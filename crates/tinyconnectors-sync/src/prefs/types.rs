//! The per-toolkit scope preference and its storage.

use serde::{Deserialize, Serialize};

use crate::scope::ToolScope;
use crate::state::SyncStateStore;
use crate::{Error, Result};

/// The key-value namespace holding one row per toolkit.
///
/// Deliberately distinct from the sync-state namespace, so a preference and a
/// cursor for the same toolkit can never collide.
pub const PREFS_NAMESPACE: &str = "composio-user-scopes";

/// Which scopes an agent may use for one toolkit.
///
/// Three independent flags rather than a single maximum level: a user who wants
/// an agent to read and delete stale mail but never send any is expressing
/// something a threshold cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserScopePref {
    /// Whether the agent may read.
    #[serde(default = "enabled")]
    pub read: bool,
    /// Whether the agent may create or change things.
    #[serde(default = "enabled")]
    pub write: bool,
    /// Whether the agent may delete or change permissions.
    #[serde(default)]
    pub admin: bool,
}

fn enabled() -> bool {
    true
}

impl Default for UserScopePref {
    /// Read and write, but not admin — see the module docs.
    fn default() -> Self {
        Self {
            read: true,
            write: true,
            admin: false,
        }
    }
}

impl UserScopePref {
    /// Whether `scope` is permitted.
    #[must_use]
    pub fn allows(self, scope: ToolScope) -> bool {
        match scope {
            ToolScope::Read => self.read,
            ToolScope::Write => self.write,
            ToolScope::Admin => self.admin,
        }
    }

    /// The row key for `toolkit` — trimmed and lowercased.
    ///
    /// Normalized because the toolkit arrives from a config file, a UI field,
    /// and a backend envelope. A key one of those spelled differently would not
    /// fail: it would read as "no preference stored" and hand the agent the
    /// default while the user's saved choice sat one key away.
    #[must_use]
    pub fn key(toolkit: &str) -> String {
        toolkit.trim().to_ascii_lowercase()
    }

    /// The preference stored for `toolkit`, or the default if none is.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Store`] when the store failed, or [`Error::Decode`]
    /// when the stored row is not this shape. Neither falls back to the
    /// default: a preference that cannot be read must not quietly become
    /// permission the user did not grant.
    pub async fn load(store: &dyn SyncStateStore, toolkit: &str) -> Result<Self> {
        let key = Self::key(toolkit);
        let Some(value) = store.get(PREFS_NAMESPACE, &key).await? else {
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
    pub async fn save(self, store: &dyn SyncStateStore, toolkit: &str) -> Result<()> {
        let value = serde_json::json!({
            "read": self.read,
            "write": self.write,
            "admin": self.admin,
        });
        store
            .set(PREFS_NAMESPACE, &Self::key(toolkit), &value)
            .await
    }
}
