//! The host-supplied key-value seam, and the state kept in it.

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::budget::DailyBudget;
use crate::{Error, Result};

/// The namespace every persisted cursor lives under.
///
/// Durable: changing it strands every user's cursor, and the next run of every
/// connection re-reads its whole history. Pinned by a test.
pub const STATE_NAMESPACE: &str = "composio-sync-state";

/// Where sync state is persisted, supplied by the host.
///
/// Two methods over JSON, on purpose — see the module docs.
#[async_trait]
pub trait SyncStateStore: Send + Sync {
    /// Read a value, or `None` if the key has never been written.
    ///
    /// # Errors
    ///
    /// Returns an error only when the store itself failed. A missing key is
    /// `Ok(None)`: a connection that has never synced is the normal first case,
    /// not a failure.
    async fn get(&self, namespace: &str, key: &str) -> Result<Option<serde_json::Value>>;

    /// Write a value, replacing any previous one.
    ///
    /// # Errors
    ///
    /// Returns an error when the store failed to persist the value.
    async fn set(&self, namespace: &str, key: &str, value: &serde_json::Value) -> Result<()>;
}

/// What one connection's sync remembers between runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncState {
    /// Toolkit slug this state belongs to.
    pub toolkit: String,
    /// Connection this state belongs to.
    pub connection_id: String,
    /// Provider pagination position to resume from.
    #[serde(default)]
    pub cursor: Option<String>,
    /// Item ids already ingested.
    #[serde(default)]
    pub synced_ids: HashSet<String>,
    /// Last seen version per item, for sources that report one.
    ///
    /// Separate from [`Self::synced_ids`] because an item can be seen before
    /// and still have changed — an edited Notion page must re-ingest, while an
    /// untouched one must not.
    #[serde(default)]
    pub item_versions: HashMap<String, String>,
    /// Today's provider request allowance.
    #[serde(default)]
    pub daily_budget: DailyBudget,
    /// Newest item id seen, for sources that page newest-first.
    #[serde(default)]
    pub last_seen_id: Option<String>,
    /// When the last run finished, in milliseconds since the Unix epoch.
    #[serde(default)]
    pub last_sync_at_ms: Option<u64>,

    /// Requests made by the run in progress.
    ///
    /// Not persisted: it describes this run, and a stored value would be
    /// double-counted against the next one.
    #[serde(skip)]
    pub run_requests: u32,
    /// Provider cost incurred by the run in progress, in USD. Not persisted,
    /// for the same reason.
    #[serde(skip)]
    pub run_cost_usd: f64,
}

impl SyncState {
    /// Fresh state for a connection that has never synced.
    #[must_use]
    pub fn new(toolkit: impl Into<String>, connection_id: impl Into<String>) -> Self {
        Self {
            toolkit: toolkit.into(),
            connection_id: connection_id.into(),
            ..Self::default()
        }
    }

    /// The store key for one connection's state.
    #[must_use]
    pub fn key(toolkit: &str, connection_id: &str) -> String {
        format!("{toolkit}:{connection_id}")
    }

    /// Whether `id` has already been ingested.
    #[must_use]
    pub fn is_synced(&self, id: &str) -> bool {
        self.synced_ids.contains(id)
    }

    /// Whether `id` needs ingesting, given the version the source reports.
    ///
    /// An item is new, or its version changed, or it is unchanged and skipped.
    /// A source that reports no version falls back to "seen means done".
    #[must_use]
    pub fn needs_ingest(&self, id: &str, version: Option<&str>) -> bool {
        match version {
            Some(version) => self.item_versions.get(id).map(String::as_str) != Some(version),
            None => !self.is_synced(id),
        }
    }

    /// Record `id` as ingested, at `version` when the source reports one.
    pub fn mark_synced(&mut self, id: impl Into<String>, version: Option<&str>) {
        let id = id.into();
        if let Some(version) = version {
            self.item_versions.insert(id.clone(), version.to_string());
        }
        self.synced_ids.insert(id);
    }

    /// Whether today's request allowance is spent.
    #[must_use]
    pub fn budget_exhausted(&self) -> bool {
        self.daily_budget.is_exhausted()
    }

    /// Charge one provider action against the budget and this run's totals.
    ///
    /// `attempts` is charged as at least one: an action that was retried cost
    /// several requests, and an action reported as zero attempts still cost the
    /// one that produced that report.
    pub fn record_action(&mut self, attempts: u32, cost_usd: f64) {
        let attempts = attempts.max(1);
        self.daily_budget.record_requests(attempts);
        self.run_requests = self.run_requests.saturating_add(attempts);
        if cost_usd.is_finite() && cost_usd > 0.0 {
            self.run_cost_usd += cost_usd;
        }
    }

    /// Load a connection's state, or fresh state if it has never synced.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Store`] when the store failed, or [`Error::Decode`]
    /// when the stored value is not this shape.
    pub async fn load(
        store: &dyn SyncStateStore,
        toolkit: &str,
        connection_id: &str,
    ) -> Result<Self> {
        let key = Self::key(toolkit, connection_id);
        let Some(value) = store.get(STATE_NAMESPACE, &key).await? else {
            return Ok(Self::new(toolkit, connection_id));
        };
        serde_json::from_value(value).map_err(|error| Error::Decode {
            key,
            message: error.to_string(),
        })
    }

    /// Persist this state.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Decode`] if the state cannot be serialized, and
    /// [`Error::Store`] when the store failed to write it.
    pub async fn save(&self, store: &dyn SyncStateStore) -> Result<()> {
        let key = Self::key(&self.toolkit, &self.connection_id);
        let value = serde_json::to_value(self).map_err(|error| Error::Decode {
            key: key.clone(),
            message: error.to_string(),
        })?;
        store.set(STATE_NAMESPACE, &key, &value).await
    }
}
