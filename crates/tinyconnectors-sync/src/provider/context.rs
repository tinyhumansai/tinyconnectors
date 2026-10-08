//! What a provider is given for one call.

use std::sync::Arc;

use async_trait::async_trait;

use crate::Result;

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
    /// provider reading an identity has nothing useful to do with a half-answer.
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

/// Everything one provider needs to read a connected account.
#[derive(Clone)]
pub struct ProviderContext {
    /// Toolkit being read.
    pub toolkit: String,
    /// Connection being read.
    pub connection_id: String,
    /// How to run Composio actions.
    pub actions: Arc<dyn ActionRunner>,
}

impl std::fmt::Debug for ProviderContext {
    /// Omits the action runner, a host implementation whose `Debug` could print
    /// anything — including a client holding a credential.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderContext")
            .field("toolkit", &self.toolkit)
            .field("connection_id", &self.connection_id)
            .finish_non_exhaustive()
    }
}

impl ProviderContext {
    /// Run an action against this context's connection.
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
