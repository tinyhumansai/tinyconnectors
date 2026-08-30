//! The host-supplied HTTP seam.

use async_trait::async_trait;

use crate::Result;

/// The three verbs a connector backend is reached through.
///
/// Implementors own authentication, timeouts, base URL, proxying, and tracing.
/// This crate contributes only the paths and the payload shapes.
///
/// Paths passed in are absolute and backend-relative, e.g.
/// `/agent-integrations/composio/toolkits`. Implementors join them to their own
/// base URL rather than receiving a full URL, so a host cannot be talked into
/// sending its credential somewhere else by a value that crossed the bus.
#[async_trait]
pub trait Transport: Send + Sync + std::fmt::Debug {
    /// Perform a GET and return the decoded response body.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::Transport`] when the request fails or the
    /// backend reports an error status.
    async fn get(&self, path: &str) -> Result<serde_json::Value>;

    /// Perform a POST with a JSON body and return the decoded response body.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::Transport`] when the request fails or the
    /// backend reports an error status.
    async fn post(&self, path: &str, body: &serde_json::Value) -> Result<serde_json::Value>;

    /// Perform a DELETE and return the decoded response body.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::Transport`] when the request fails or the
    /// backend reports an error status.
    async fn delete(&self, path: &str) -> Result<serde_json::Value>;
}
