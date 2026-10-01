//! Applying a host's network policy to the HTTP client.
//!
//! The module cannot read the host's proxy settings or trust store, so the host
//! resolves its policy for this service to a [`ComposioTransportConfig`] and
//! hands it over (in the route description, or on a direct credential). This
//! turns that description into a `ureq` agent.
//!
//! # What the policy decides
//!
//! - **Proxy.** One proxy URL, honoured for every request the agent makes. A
//!   destination matching the `no_proxy` list goes direct. The match follows
//!   the usual `NO_PROXY` reading: `*`, an exact host or any subdomain of it
//!   (`example.com`, `.example.com`, `*.example.com`), an IP literal, or an IPv4
//!   or IPv6 CIDR range.
//! - **Roots.** The bundled Mozilla roots, or the operating system's store.
//!
//! # What it never relaxes
//!
//! The HTTPS-or-loopback guard on the base URL lives in `HttpTransport`, before
//! any agent exists, and a proxy does not change it: a proxy carries the
//! request, it does not make plain HTTP to a remote host acceptable.

use std::net::IpAddr;
use std::time::Duration;

use tinyconnectors_bus::{ComposioTlsRoots, ComposioTransportConfig};
use ureq::config::Config;
use ureq::tls::{RootCerts, TlsConfig};
use ureq::{Agent, Proxy, ProxyProtocol};

use crate::{Error, Result};

/// How a transport treats the responses it gets back.
///
/// The credentialed direct route must not follow a redirect, because the
/// `x-api-key` header would follow it, and it reports a failing status with the
/// provider's own message. The backend route keeps its established behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Strictness {
    /// Follow redirects; a failing status is the HTTP client's own error.
    Backend,
    /// Never follow a redirect; a failing status is returned to be rendered.
    Credentialed,
}

/// Build the agent for `scheme`, applying `network` to a request for `host`.
///
/// # Errors
///
/// Returns [`Error::InvalidNetworkConfig`] when the proxy URL cannot be used.
pub(super) fn build_agent(
    timeout: Duration,
    strictness: Strictness,
    network: Option<&ComposioTransportConfig>,
    host: &str,
) -> Result<Agent> {
    let mut config = Config::builder().timeout_global(Some(timeout));
    if strictness == Strictness::Credentialed {
        config = config
            .max_redirects(0)
            .max_redirects_will_error(false)
            .http_status_as_error(false);
    }
    if let Some(network) = network {
        // No proxy URL means the host's policy leaves proxying to the process
        // environment, which is the library's default and so is left alone. A
        // proxy that the destination bypasses is different: the host chose one,
        // and the answer for this destination is "none", not "whatever the
        // environment says".
        match proxy_for(network, host)? {
            ProxyChoice::Environment => {}
            ProxyChoice::Bypass => config = config.proxy(None),
            ProxyChoice::Use(proxy) => config = config.proxy(Some(proxy)),
        }
        if network.tls_roots == ComposioTlsRoots::Platform {
            config = config.tls_config(
                TlsConfig::builder()
                    .root_certs(RootCerts::PlatformVerifier)
                    .build(),
            );
        }
    }
    Ok(config.build().into())
}

/// What the host's policy says about proxying a request to one host.
enum ProxyChoice {
    /// No proxy configured: the library's environment default applies.
    Environment,
    /// A proxy is configured and this destination is exempt from it.
    Bypass,
    /// Route through this proxy.
    Use(Proxy),
}

fn proxy_for(network: &ComposioTransportConfig, host: &str) -> Result<ProxyChoice> {
    let Some(raw) = network
        .proxy_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
    else {
        return Ok(ProxyChoice::Environment);
    };
    if bypasses_proxy(&network.no_proxy, host) {
        tracing::debug!("[connectors][network] destination matches no_proxy; going direct");
        return Ok(ProxyChoice::Bypass);
    }
    parse_proxy(raw).map(ProxyChoice::Use)
}

fn invalid(reason: &'static str) -> Error {
    Error::InvalidNetworkConfig { reason }
}

fn parse_proxy(raw: &str) -> Result<Proxy> {
    let parsed = url::Url::parse(raw).map_err(|_| invalid("the proxy URL is not a valid URL"))?;
    let protocol = match parsed.scheme() {
        "http" => ProxyProtocol::Http,
        "https" => ProxyProtocol::Https,
        "socks4" => ProxyProtocol::Socks4,
        "socks4a" => ProxyProtocol::Socks4A,
        "socks5" | "socks" => ProxyProtocol::Socks5,
        "socks5h" => ProxyProtocol::Socks5h,
        _ => return Err(invalid("the proxy URL scheme is not supported")),
    };
    let host = parsed
        .host_str()
        .ok_or_else(|| invalid("the proxy URL has no host"))?;
    let mut builder = Proxy::builder(protocol).host(host);
    if let Some(port) = parsed.port() {
        builder = builder.port(port);
    }
    if !parsed.username().is_empty() {
        let user = decode(parsed.username());
        builder = builder.username(&user);
        if let Some(password) = parsed.password() {
            builder = builder.password(&decode(password));
        }
    }
    builder
        .build()
        .map_err(|_| invalid("the proxy URL cannot be used"))
}

fn decode(component: &str) -> String {
    percent_encoding::percent_decode_str(component)
        .decode_utf8_lossy()
        .into_owned()
}

/// Whether a request to `host` skips the proxy under `entries`.
///
pub(super) fn bypasses_proxy(entries: &[String], host: &str) -> bool {
    let host = host
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    let ip = host.parse::<IpAddr>().ok();
    entries
        .iter()
        .flat_map(|entry| entry.split(','))
        .map(|entry| entry.trim().to_ascii_lowercase())
        .filter(|entry| !entry.is_empty())
        .any(|entry| entry_matches(&entry, &host, ip))
}

fn entry_matches(entry: &str, host: &str, ip: Option<IpAddr>) -> bool {
    if entry == "*" {
        return true;
    }
    if let Some((network, bits)) = entry.split_once('/') {
        return match (network.parse::<IpAddr>(), bits.parse::<u8>(), ip) {
            (Ok(network), Ok(bits), Some(ip)) => in_cidr(network, bits, ip),
            _ => false,
        };
    }
    if let Ok(literal) = entry.trim_matches(['[', ']']).parse::<IpAddr>() {
        return ip == Some(literal);
    }
    // A domain matches itself and every subdomain of it. A leading `*.` or `.`
    // is the same statement written the other common way.
    let domain = entry.trim_start_matches('*').trim_start_matches('.');
    !domain.is_empty() && (host == domain || host.ends_with(&format!(".{domain}")))
}

fn in_cidr(network: IpAddr, bits: u8, ip: IpAddr) -> bool {
    match (network, ip) {
        (IpAddr::V4(network), IpAddr::V4(ip)) if bits <= 32 => prefix_matches(
            u128::from(u32::from(network)),
            u128::from(u32::from(ip)),
            32,
            bits,
        ),
        (IpAddr::V6(network), IpAddr::V6(ip)) if bits <= 128 => {
            prefix_matches(u128::from(network), u128::from(ip), 128, bits)
        }
        _ => false,
    }
}

fn prefix_matches(network: u128, ip: u128, width: u32, bits: u8) -> bool {
    let bits = u32::from(bits);
    if bits == 0 {
        return true;
    }
    let shift = width - bits;
    (network >> shift) == (ip >> shift)
}

#[cfg(test)]
#[path = "network_tests.rs"]
mod test;
