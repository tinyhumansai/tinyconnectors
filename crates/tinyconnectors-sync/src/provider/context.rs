//! What a provider is given for one sync run.

use std::sync::Arc;

use async_trait::async_trait;

use crate::Result;
use crate::state::SyncStateStore;

/// Runs one Composio action and returns the provider's JSON.
///
/// The seam that lets this crate call actions without depending on the module
/// that knows how to reach Composio. One method, because that is all a provider
/// does: everything else it needs it computes.
#[async_trait]
pub trait ActionRunner: Send + Sync + std::fmt::Debug {
    /// Run `action` with `arguments` against `connection_id`.
    ///
    /// Returns the provider's payload — the `data` of an execute response, not
    /// the envelope. An action the provider refused is an error here, because a
    /// provider reading a page has nothing useful to do with a half-answer.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::Action`] when the action could not be run or the
    /// provider reported a failure.
    async fn run(
        &self,
        action: &str,
        arguments: serde_json::Value,
        connection_id: &str,
    ) -> Result<serde_json::Value>;
}

/// Bounds a sync run must respect.
///
/// These exist because a first sync of a years-old mailbox is otherwise
/// unbounded: it costs money per request, takes hours, and buries whatever the
/// user actually wanted in a backfill they did not ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncLimits {
    /// Most records to produce in one run.
    pub max_items: usize,
    /// How far back to read, in days. `None` means no lower bound.
    pub depth_days: Option<u32>,
}

impl Default for SyncLimits {
    /// Two hundred items and ninety days.
    ///
    /// Chosen to be a useful first sync rather than a complete one: enough that
    /// the agent has real context immediately, small enough that it finishes
    /// while the user is still looking at the screen. Later runs resume from
    /// the cursor, so nothing is lost by starting small.
    fn default() -> Self {
        Self {
            max_items: 200,
            depth_days: Some(90),
        }
    }
}

/// Everything one provider needs for one sync run.
#[derive(Clone)]
pub struct ProviderContext {
    /// Toolkit being synced.
    pub toolkit: String,
    /// Connection being read.
    pub connection_id: String,
    /// Memory source the records belong to.
    pub source_id: String,
    /// Bounds this run must respect.
    pub limits: SyncLimits,
    /// How to run Composio actions.
    pub actions: Arc<dyn ActionRunner>,
    /// Where cursors and budgets live.
    pub state: Arc<dyn SyncStateStore>,
}

impl std::fmt::Debug for ProviderContext {
    /// Omits the two seams, which are host implementations whose `Debug` could
    /// print anything — including, in the case of an action runner, a client
    /// holding a credential.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderContext")
            .field("toolkit", &self.toolkit)
            .field("connection_id", &self.connection_id)
            .field("source_id", &self.source_id)
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

impl ProviderContext {
    /// Run an action against this run's connection.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::Action`] when the action fails.
    pub async fn run(
        &self,
        action: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value> {
        self.actions
            .run(action, arguments, &self.connection_id)
            .await
    }
}
