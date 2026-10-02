//! Unit tests for the network policy: the no-proxy rule and proxy URL parsing.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use tinyconnectors_bus::{ComposioTlsRoots, ComposioTransportConfig};

use super::{Strictness, build_agent, bypasses_proxy, parse_proxy};
use crate::Error;

fn list(entries: &[&str]) -> Vec<String> {
    entries.iter().map(ToString::to_string).collect()
}

#[test]
fn an_empty_list_bypasses_nothing() {
    assert!(!bypasses_proxy(&[], "backend.composio.dev"));
}

#[test]
fn a_star_bypasses_everything() {
    assert!(bypasses_proxy(&list(&["*"]), "backend.composio.dev"));
}

#[test]
fn a_domain_matches_itself_and_its_subdomains_only() {
    let entries = list(&["composio.dev"]);
    assert!(bypasses_proxy(&entries, "composio.dev"));
    assert!(bypasses_proxy(&entries, "backend.composio.dev"));
    assert!(bypasses_proxy(&entries, "BACKEND.Composio.Dev"));
    assert!(!bypasses_proxy(&entries, "notcomposio.dev"));
    assert!(!bypasses_proxy(&entries, "composio.dev.evil.example"));
}

#[test]
fn a_leading_dot_or_star_dot_is_the_same_domain() {
    for entry in [".composio.dev", "*.composio.dev"] {
        let entries = list(&[entry]);
        assert!(bypasses_proxy(&entries, "backend.composio.dev"), "{entry}");
        assert!(!bypasses_proxy(&entries, "other.dev"), "{entry}");
    }
}

#[test]
fn entries_may_be_comma_separated_and_padded() {
    let entries = list(&[" localhost , .internal ", ""]);
    assert!(bypasses_proxy(&entries, "localhost"));
    assert!(bypasses_proxy(&entries, "api.internal"));
    assert!(!bypasses_proxy(&entries, "api.example"));
}

#[test]
fn ip_literals_match_exactly_and_by_cidr() {
    let entries = list(&["127.0.0.1", "10.0.0.0/8", "fd00::/8", "::1"]);
    assert!(bypasses_proxy(&entries, "127.0.0.1"));
    assert!(!bypasses_proxy(&entries, "127.0.0.2"));
    assert!(bypasses_proxy(&entries, "10.200.3.4"));
    assert!(!bypasses_proxy(&entries, "11.0.0.1"));
    assert!(bypasses_proxy(&entries, "fd12::1"));
    assert!(bypasses_proxy(&entries, "[::1]"));
    assert!(!bypasses_proxy(&entries, "example.com"));
}

#[test]
fn a_cidr_never_matches_a_hostname_or_the_other_family() {
    let entries = list(&[
        "10.0.0.0/8",
        "0.0.0.0/0",
        "fd00::/200",
        "10.0.0.0/abc",
        "bad/8",
    ]);
    assert!(!bypasses_proxy(&list(&["10.0.0.0/8"]), "ten.example"));
    assert!(!bypasses_proxy(&list(&["10.0.0.0/8"]), "fd00::1"));
    assert!(
        bypasses_proxy(&entries, "192.168.1.1"),
        "/0 matches every IPv4"
    );
    assert!(!bypasses_proxy(
        &list(&["fd00::/200", "10.0.0.0/abc", "bad/8"]),
        "fd00::1"
    ));
}

#[test]
fn proxy_urls_of_every_supported_scheme_parse() {
    for url in [
        "http://127.0.0.1:8080",
        "https://proxy.internal",
        "socks5://proxy.internal:1080",
        "socks5h://proxy.internal:1080",
        "socks4://proxy.internal:1080",
        "socks4a://proxy.internal:1080",
        "http://user:p%40ss@proxy.internal:3128",
    ] {
        assert!(parse_proxy(url).is_ok(), "{url}");
    }
}

#[test]
fn a_proxy_url_that_cannot_be_used_is_refused_without_quoting_it() {
    for url in ["not a url", "ftp://proxy.internal", "http://"] {
        let error = parse_proxy(url).unwrap_err();
        assert!(matches!(error, Error::InvalidNetworkConfig { .. }), "{url}");
        assert!(!error.to_string().contains(url), "{error}");
    }
    let secret = parse_proxy("ftp://user:hunter2@proxy.internal").unwrap_err();
    assert!(!secret.to_string().contains("hunter2"));
}

fn agent(network: Option<&ComposioTransportConfig>) -> crate::Result<ureq::Agent> {
    build_agent(
        Duration::from_secs(5),
        Strictness::Credentialed,
        network,
        "example.com",
    )
}

#[test]
fn an_agent_builds_for_each_policy_shape() {
    assert!(agent(None).is_ok());
    assert!(agent(Some(&ComposioTransportConfig::default())).is_ok());
    assert!(
        build_agent(
            Duration::from_secs(5),
            Strictness::Backend,
            None,
            "example.com"
        )
        .is_ok()
    );
    let proxied = ComposioTransportConfig {
        proxy_url: Some("socks5://127.0.0.1:1080".to_string()),
        no_proxy: list(&["example.com"]),
        ..Default::default()
    };
    assert!(agent(Some(&proxied)).is_ok(), "bypassed destination");
    let platform = ComposioTransportConfig {
        tls_roots: ComposioTlsRoots::Platform,
        ..Default::default()
    };
    assert!(agent(Some(&platform)).is_ok());
}

#[test]
fn a_bad_proxy_url_fails_the_agent_build() {
    let bad = ComposioTransportConfig {
        proxy_url: Some("ftp://nope".to_string()),
        ..Default::default()
    };
    assert!(matches!(
        agent(Some(&bad)),
        Err(Error::InvalidNetworkConfig { .. })
    ));
    // A blank proxy URL is "no proxy", not an error.
    let blank = ComposioTransportConfig {
        proxy_url: Some("   ".to_string()),
        ..Default::default()
    };
    assert!(agent(Some(&blank)).is_ok());
}
