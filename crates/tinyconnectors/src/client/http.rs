//! An HTTP [`Transport`] over the host-supplied backend URL and credential.

use std::time::Duration;

use async_trait::async_trait;

use super::transport::Transport;
use crate::{Error, Result};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// How a transport presents its credential.
///
/// The two routes authenticate differently — the TinyHumans backend takes a
/// user session as `Authorization: Bearer`, Composio takes a user-supplied key
/// as `x-api-key` — so the header is transport configuration rather than
/// something a caller passes per request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthScheme {
    Bearer,
    ApiKey,
}

impl AuthScheme {
    fn header(self) -> &'static str {
        match self {
            Self::Bearer => "authorization",
            Self::ApiKey => "x-api-key",
        }
    }

    fn value(self, credential: &str) -> String {
        match self {
            Self::Bearer => format!("Bearer {credential}"),
            Self::ApiKey => credential.to_string(),
        }
    }
}

/// A [`Transport`] that reaches a base URL over HTTPS with a credential.
///
/// The credential is held here and nowhere else: it is never logged, never
/// returned through a bus member, and never interpolated into a path. Paths
/// arrive base-relative and are joined to `base_url`, so a value that crossed
/// the bus cannot redirect a credentialed request at another host.
pub struct HttpTransport {
    base_url: String,
    credential: String,
    scheme: AuthScheme,
    agent: ureq::Agent,
}

impl std::fmt::Debug for HttpTransport {
    /// Deliberately omits the credential. A `Debug` derive here would print the
    /// user's token into any log line that formats the transport.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpTransport")
            .field("base_url", &self.base_url)
            .field("scheme", &self.scheme)
            .finish_non_exhaustive()
    }
}

impl HttpTransport {
    /// Build a transport that sends `Authorization: Bearer <token>`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InsecureBaseUrl`] if `base_url` would send the
    /// credential anywhere other than an HTTPS host or a genuine loopback
    /// address — see [`check_base_url`].
    pub fn bearer(base_url: &str, token: impl Into<String>) -> Result<Self> {
        Self::build(base_url, token.into(), AuthScheme::Bearer)
    }

    /// Build a transport that sends `x-api-key: <key>`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InsecureBaseUrl`] on a base URL that would leak the
    /// key — see [`check_base_url`].
    pub fn api_key(base_url: &str, key: impl Into<String>) -> Result<Self> {
        Self::build(base_url, key.into(), AuthScheme::ApiKey)
    }

    /// A trailing slash on `base_url` is trimmed so joining a leading-slash
    /// path cannot produce a double slash the backend routes differently.
    fn build(base_url: &str, credential: String, scheme: AuthScheme) -> Result<Self> {
        let base_url = base_url.trim().trim_end_matches('/').to_string();
        check_base_url(&base_url)?;
        Ok(Self {
            base_url,
            credential,
            scheme,
            agent: ureq::Agent::config_builder()
                .timeout_global(Some(REQUEST_TIMEOUT))
                .build()
                .into(),
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn auth_header(&self) -> (&'static str, String) {
        (self.scheme.header(), self.scheme.value(&self.credential))
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
        let (header, value) = self.auth_header();
        self.call(path, move || {
            agent
                .get(&url)
                .header(header, &value)
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
        let (header, value) = self.auth_header();
        let body = body.clone();
        self.call(path, move || {
            agent
                .post(&url)
                .header(header, &value)
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
        let (header, value) = self.auth_header();
        self.call(path, move || {
            agent
                .delete(&url)
                .header(header, &value)
                .call()
                .map_err(|error| error.to_string())?
                .body_mut()
                .read_to_string()
                .map_err(|error| error.to_string())
        })
        .await
    }
}

/// Refuse a base URL that would send a credential where it must not go.
///
/// HTTPS, or a genuine loopback address for local development. The check parses
/// rather than prefix-matches, because a `starts_with("http://127.0.0.1")` test
/// is fooled by userinfo smuggling: `http://127.0.0.1:8080@evil.com/api` has
/// host `evil.com`, and an HTTP client routes it there — carrying the
/// credential header with it. Embedded credentials are rejected outright for
/// the same reason.
fn check_base_url(base_url: &str) -> Result<()> {
    let insecure = |reason| Error::InsecureBaseUrl {
        base_url: base_url.to_string(),
        reason,
    };

    let parsed = url::Url::parse(base_url).map_err(|_| insecure("not a valid URL"))?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(insecure("URL carries embedded credentials"));
    }
    match parsed.scheme() {
        "https" => Ok(()),
        "http" => {
            let loopback = match parsed.host() {
                Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
                Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
                Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
                None => false,
            };
            if loopback {
                Ok(())
            } else {
                Err(insecure("plain HTTP is only allowed to a loopback address"))
            }
        }
        _ => Err(insecure("URL scheme must be https")),
    }
}

#[cfg(test)]
#[path = "http_test.rs"]
mod test;
