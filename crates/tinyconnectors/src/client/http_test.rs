//! Unit tests for the HTTP transport's base-URL guard.
//!
//! These are the tests that matter most in this file: everything else here
//! sends a request, but this decides whether a user's credential is allowed to
//! leave the machine at all.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{HttpTransport, check_base_url};
use crate::Error;

#[test]
fn accepts_https() {
    assert!(check_base_url("https://api.example.com").is_ok());
    assert!(check_base_url("https://backend.composio.dev/api/v3").is_ok());
}

#[test]
fn accepts_plain_http_only_to_a_real_loopback_address() {
    for base in [
        "http://localhost:8080",
        "http://127.0.0.1:8080",
        "http://[::1]:8080",
    ] {
        assert!(check_base_url(base).is_ok(), "{base} should be allowed");
    }
}

#[test]
fn rejects_plain_http_to_anywhere_else() {
    let error = check_base_url("http://api.example.com").unwrap_err();
    assert!(matches!(error, Error::InsecureBaseUrl { .. }));
    assert!(error.to_string().contains("loopback"));
}

#[test]
fn rejects_userinfo_smuggling_that_looks_like_loopback() {
    // The whole reason this is a parse and not a prefix match. The host here is
    // `evil.com`, and a prefix check on "http://127.0.0.1:" would wave it
    // through — sending the credential header straight to the attacker.
    for base in [
        "http://127.0.0.1:8080@evil.com/api",
        "http://localhost@evil.com/api",
        "https://user:pass@evil.com/api",
    ] {
        let error = check_base_url(base).unwrap_err();
        assert!(
            error.to_string().contains("embedded credentials"),
            "{base} must be refused for its userinfo: {error}"
        );
    }
}

#[test]
fn rejects_a_non_http_scheme() {
    for base in ["file:///etc/passwd", "ftp://example.com"] {
        let error = check_base_url(base).unwrap_err();
        assert!(error.to_string().contains("https"), "{base}: {error}");
    }
}

#[test]
fn rejects_something_that_is_not_a_url() {
    let error = check_base_url("not a url").unwrap_err();
    assert!(error.to_string().contains("valid URL"));
}

#[test]
fn constructors_refuse_an_unsafe_base_url() {
    assert!(HttpTransport::bearer("http://evil.com", "token").is_err());
    assert!(HttpTransport::api_key("http://evil.com", "key").is_err());
}

#[test]
fn trims_a_trailing_slash_so_joined_paths_have_one_separator() {
    let transport = HttpTransport::bearer("https://api.example.com/", "token").unwrap();
    assert_eq!(
        transport.url("/agent-integrations/composio/toolkits"),
        "https://api.example.com/agent-integrations/composio/toolkits"
    );
}

#[test]
fn debug_output_never_carries_the_credential() {
    let bearer = HttpTransport::bearer("https://api.example.com", "super-secret-token").unwrap();
    let rendered = format!("{bearer:?}");
    assert!(!rendered.contains("super-secret-token"), "{rendered}");
    assert!(rendered.contains("api.example.com"));

    let direct =
        HttpTransport::api_key("https://backend.composio.dev/api/v3", "sk-secret").unwrap();
    let rendered = format!("{direct:?}");
    assert!(!rendered.contains("sk-secret"), "{rendered}");
}

#[test]
fn each_scheme_sends_the_header_its_route_expects() {
    let bearer = HttpTransport::bearer("https://api.example.com", "t0ken").unwrap();
    assert_eq!(
        bearer.auth_header(),
        ("authorization", "Bearer t0ken".into())
    );

    let direct = HttpTransport::api_key("https://backend.composio.dev/api/v3", "sk-1").unwrap();
    assert_eq!(direct.auth_header(), ("x-api-key", "sk-1".into()));
}
