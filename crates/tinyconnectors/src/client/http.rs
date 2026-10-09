//! An HTTP [`Transport`] over the host-supplied backend URL and credential.

use std::time::Duration;

use async_trait::async_trait;

use tinyconnectors_bus::ComposioTransportConfig;

use super::network::{Strictness, build_agent};
use super::transport::Transport;
use crate::{Error, Result};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Header naming the user's IANA time zone, e.g. `Asia/Kolkata`.
///
/// Upstream results carry timestamps in UTC. The backend renders them for a
/// model to read, and a model handed only `12:09:20Z` plus a local-time clock
/// in its prompt reads the UTC digits as local time. With the zone the backend
/// can print the local time beside the UTC value.
pub(crate) const TIMEZONE_HEADER: &str = "x-timezone";

/// Longest accepted zone name. The longest IANA name is well under this; a
/// longer value is a misconfiguration, dropped rather than sent.
const TIMEZONE_MAX_LEN: usize = 64;

/// How a transport presents its credential.
///
/// The two routes authenticate differently — the `TinyHumans` backend takes a
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

    /// How strictly the transport treats a response. A credential sent as
    /// `x-api-key` must never follow a redirect, and its failures carry the
    /// provider's own message; the backend bearer keeps the established
    /// behaviour.
    fn strictness(self) -> Strictness {
        match self {
            Self::Bearer => Strictness::Backend,
            Self::ApiKey => Strictness::Credentialed,
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
    /// Sent as `TIMEZONE_HEADER` when set. See [`HttpTransport::with_timezone`].
    timezone: Option<String>,
    agent: ureq::Agent,
}

impl std::fmt::Debug for HttpTransport {
    /// Deliberately omits the credential. A `Debug` derive here would print the
    /// user's token into any log line that formats the transport.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpTransport")
            .field("base_url", &self.base_url)
            .field("scheme", &self.scheme)
            .field("timezone", &self.timezone)
            .finish_non_exhaustive()
    }
}

impl HttpTransport {
    /// Build a transport that sends `Authorization: Bearer <token>`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InsecureBaseUrl`] unless `base_url` is HTTPS or a
    /// genuine loopback address, or if it carries embedded credentials.
    pub fn bearer(base_url: &str, token: impl Into<String>) -> Result<Self> {
        Self::build(base_url, token.into(), AuthScheme::Bearer)
    }

    /// Build a transport that sends `x-api-key: <key>`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InsecureBaseUrl`] on a base URL that would leak the
    /// key: anything but HTTPS or a genuine loopback address.
    pub fn api_key(base_url: &str, key: impl Into<String>) -> Result<Self> {
        Self::build(base_url, key.into(), AuthScheme::ApiKey)
    }

    /// A trailing slash on `base_url` is trimmed so joining a leading-slash
    /// path cannot produce a double slash the backend routes differently.
    fn build(base_url: &str, credential: String, scheme: AuthScheme) -> Result<Self> {
        let base_url = base_url.trim().trim_end_matches('/').to_string();
        let host = check_base_url(&base_url)?;
        Ok(Self {
            agent: build_agent(REQUEST_TIMEOUT, scheme.strictness(), None, &host)?,
            base_url,
            credential,
            scheme,
            timezone: None,
        })
    }

    /// Apply the host's proxy and TLS policy to this transport.
    ///
    /// The base-URL guard has already run and is not affected: a proxy carries
    /// the request, it does not make plain HTTP to a remote host acceptable.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidNetworkConfig`] when the proxy URL cannot be
    /// used. Nothing is sent in that case.
    pub fn with_network(mut self, network: Option<&ComposioTransportConfig>) -> Result<Self> {
        let host = check_base_url(&self.base_url)?;
        self.agent = build_agent(REQUEST_TIMEOUT, self.scheme.strictness(), network, &host)?;
        Ok(self)
    }

    /// Send the user's IANA time zone as `TIMEZONE_HEADER` on every request.
    ///
    /// Only IANA-shaped names are kept (`Area/Location`, letters, digits and
    /// `/ _ + -`), so a value that crossed the bus cannot inject a header or
    /// smuggle anything else. Anything else, or nothing, sends no header, and
    /// the backend keeps rendering UTC as it always has.
    #[must_use]
    pub fn with_timezone(mut self, timezone: Option<&str>) -> Self {
        self.timezone = timezone.map(str::trim).and_then(|zone| {
            let valid = !zone.is_empty()
                && zone.len() <= TIMEZONE_MAX_LEN
                && zone
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '+' | '-'));
            valid.then(|| zone.to_string())
        });
        self
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
        let timezone = self.timezone.clone();
        let strictness = self.scheme.strictness();
        self.call(path, move || {
            let mut request = agent.get(&url).header(header, &value);
            if let Some(zone) = &timezone {
                request = request.header(TIMEZONE_HEADER, zone);
            }
            finish(&url, strictness, request.call())
        })
        .await
    }

    async fn post(&self, path: &str, body: &serde_json::Value) -> Result<serde_json::Value> {
        let url = self.url(path);
        let agent = self.agent.clone();
        let (header, value) = self.auth_header();
        let body = body.clone();
        let timezone = self.timezone.clone();
        let strictness = self.scheme.strictness();
        self.call(path, move || {
            let mut request = agent.post(&url).header(header, &value);
            if let Some(zone) = &timezone {
                request = request.header(TIMEZONE_HEADER, zone);
            }
            finish(&url, strictness, request.send_json(&body))
        })
        .await
    }

    async fn delete(&self, path: &str) -> Result<serde_json::Value> {
        let url = self.url(path);
        let agent = self.agent.clone();
        let (header, value) = self.auth_header();
        let timezone = self.timezone.clone();
        let strictness = self.scheme.strictness();
        self.call(path, move || {
            let mut request = agent.delete(&url).header(header, &value);
            if let Some(zone) = &timezone {
                request = request.header(TIMEZONE_HEADER, zone);
            }
            finish(&url, strictness, request.call())
        })
        .await
    }
}

/// Turn one request's outcome into its body, or a failure message.
///
/// The backend route keeps the HTTP client's own wording. The credentialed
/// route reports what the provider said instead, because the caller acts on it:
/// a rejected key is told apart from an outage by `HTTP 401: Invalid API key`,
/// not by a bare "status 401". Transport failures there read as
/// `error sending request for url (..)`, the phrase failure classification
/// already keys on.
fn finish(
    url: &str,
    strictness: Strictness,
    result: std::result::Result<ureq::http::Response<ureq::Body>, ureq::Error>,
) -> std::result::Result<String, String> {
    if strictness == Strictness::Backend {
        return result
            .map_err(|error| error.to_string())?
            .body_mut()
            .read_to_string()
            .map_err(|error| error.to_string());
    }
    let mut response =
        result.map_err(|error| format!("error sending request for url ({url}): {error}"))?;
    let status = response.status().as_u16();
    let body = response.body_mut().read_to_string();
    if (200..300).contains(&status) {
        return body.map_err(|error| error.to_string());
    }
    Err(status_message(status, &body.unwrap_or_default()))
}

/// Longest provider message kept in a failure.
const ERROR_MESSAGE_MAX_CHARS: usize = 240;

/// Field names a provider message may echo that identify the user's data.
const REDACTED_MARKERS: [&str; 6] = [
    "connected_account_id",
    "connectedAccountId",
    "entity_id",
    "entityId",
    "user_id",
    "userId",
];

/// `HTTP <status>`, plus the provider's own message when the body carries one
/// (`{"error":{"message":..}}` or `{"message":..}`) and its `suggested_fix`
/// when it has one, each scrubbed of identifiers and bounded in length.
/// Anything else about the body is dropped.
fn status_message(status: u16, body: &str) -> String {
    let Some((message, fix)) = api_error_message(body) else {
        return format!("HTTP {status}");
    };
    let mut out = format!("HTTP {status}: {}", sanitize(&message));
    if let Some(fix) = fix {
        out.push_str(" Suggested fix: ");
        out.push_str(&sanitize(&fix));
    }
    out
}

fn sanitize(text: &str) -> String {
    let mut sanitized = text.replace('\n', " ");
    for marker in REDACTED_MARKERS {
        sanitized = sanitized.replace(marker, "[redacted]");
    }
    truncate(&sanitized, ERROR_MESSAGE_MAX_CHARS)
}

fn api_error_message(body: &str) -> Option<(String, Option<String>)> {
    let parsed: serde_json::Value = serde_json::from_str(body).ok()?;
    let error = parsed.get("error");
    let message = error
        .and_then(|error| error.get("message"))
        .and_then(serde_json::Value::as_str)
        .or_else(|| parsed.get("message").and_then(serde_json::Value::as_str))?
        .to_string();
    let fix = error
        .and_then(|error| error.get("suggested_fix"))
        .or_else(|| parsed.get("suggested_fix"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|fix| !fix.is_empty())
        .map(ToString::to_string);
    Some((message, fix))
}

fn truncate(text: &str, max_chars: usize) -> String {
    match text.char_indices().nth(max_chars) {
        Some((index, _)) => format!("{}...", text[..index].trim_end()),
        None => text.to_string(),
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
fn check_base_url(base_url: &str) -> Result<String> {
    let insecure = |reason| Error::InsecureBaseUrl {
        base_url: base_url.to_string(),
        reason,
    };

    let parsed = url::Url::parse(base_url).map_err(|_| insecure("not a valid URL"))?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(insecure("URL carries embedded credentials"));
    }
    let host = parsed.host_str().unwrap_or_default().to_string();
    match parsed.scheme() {
        "https" => Ok(host),
        "http" => {
            let loopback = match parsed.host() {
                Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
                Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
                Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
                None => false,
            };
            if loopback {
                Ok(host)
            } else {
                Err(insecure("plain HTTP is only allowed to a loopback address"))
            }
        }
        _ => Err(insecure("URL scheme must be https")),
    }
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod test;
