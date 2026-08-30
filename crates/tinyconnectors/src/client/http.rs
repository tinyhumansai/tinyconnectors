//! An HTTP [`Transport`] over the host-supplied backend URL and credential.

use std::time::Duration;

use async_trait::async_trait;

use super::transport::Transport;
use crate::{Error, Result};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// A [`Transport`] that reaches the backend over HTTPS with a bearer token.
///
/// The token is held here and nowhere else: it is never logged, never returned
/// through a bus member, and never interpolated into a path. Paths arrive
/// backend-relative and are joined to `base_url`, so a value that crossed the
/// bus cannot redirect a credentialed request at another host.
pub struct HttpTransport {
    base_url: String,
    token: String,
    agent: ureq::Agent,
}

impl std::fmt::Debug for HttpTransport {
    /// Deliberately omits the token. A `Debug` derive here would print the
    /// user's credential into any log line that formats the client.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpTransport")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

impl HttpTransport {
    /// Build a transport against `base_url`, authenticating with `token`.
    ///
    /// A trailing slash on `base_url` is trimmed so joining a leading-slash
    /// path cannot produce a double slash the backend routes differently.
    #[must_use]
    pub fn new(base_url: impl Into<String>, token: impl Into<String>) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        Self {
            base_url,
            token: token.into(),
            agent: ureq::Agent::config_builder()
                .timeout_global(Some(REQUEST_TIMEOUT))
                .build()
                .into(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Run one blocking request off the async runtime.
    ///
    /// `ureq` is blocking, and the module owns a small Tokio runtime whose
    /// worker would otherwise stall for the whole round trip.
    async fn call<F>(&self, path: &str, request: F) -> Result<serde_json::Value>
    where
        F: FnOnce() -> std::result::Result<String, String> + Send + 'static,
    {
        let path_for_error = path.to_string();
        let body = tokio::task::spawn_blocking(request)
            .await
            .map_err(|error| Error::Transport {
                path: path_for_error.clone(),
                message: format!("request task failed: {error}"),
            })?
            .map_err(|message| Error::Transport {
                path: path_for_error.clone(),
                message,
            })?;

        serde_json::from_str(&body).map_err(|error| Error::Decode {
            path: path_for_error,
            message: error.to_string(),
        })
    }
}

#[async_trait]
impl Transport for HttpTransport {
    async fn get(&self, path: &str) -> Result<serde_json::Value> {
        let url = self.url(path);
        let agent = self.agent.clone();
        let token = self.token.clone();
        self.call(path, move || {
            agent
                .get(&url)
                .header("authorization", &format!("Bearer {token}"))
                .call()
                .map_err(|error| error.to_string())?
                .body_mut()
                .read_to_string()
                .map_err(|error| error.to_string())
        })
        .await
    }

    async fn post(&self, path: &str, body: &serde_json::Value) -> Result<serde_json::Value> {
        let url = self.url(path);
        let agent = self.agent.clone();
        let token = self.token.clone();
        let body = body.clone();
        self.call(path, move || {
            agent
                .post(&url)
                .header("authorization", &format!("Bearer {token}"))
                .send_json(&body)
                .map_err(|error| error.to_string())?
                .body_mut()
                .read_to_string()
                .map_err(|error| error.to_string())
        })
        .await
    }

    async fn delete(&self, path: &str) -> Result<serde_json::Value> {
        let url = self.url(path);
        let agent = self.agent.clone();
        let token = self.token.clone();
        self.call(path, move || {
            agent
                .delete(&url)
                .header("authorization", &format!("Bearer {token}"))
                .call()
                .map_err(|error| error.to_string())?
                .body_mut()
                .read_to_string()
                .map_err(|error| error.to_string())
        })
        .await
    }
}
