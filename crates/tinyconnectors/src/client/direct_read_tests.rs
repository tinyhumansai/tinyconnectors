//! Tests for the stateless direct reads, against a loopback Composio.
//!
//! The proxy tests put a real CONNECT proxy on loopback between the module and
//! the server, so what they assert is that traffic actually went through it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::net::TcpListener;

use tinyconnectors_bus::{ComposioDirectCredential, ComposioTransportConfig};

use super::{list_connections, list_tools};
use crate::client::test_support::{connect_proxy, json, server};

fn credential(base_url: &str) -> ComposioDirectCredential {
    ComposioDirectCredential {
        api_key: "ck_test_key".to_string(),
        entity_id: None,
        base_url: Some(base_url.to_string()),
        transport: None,
    }
}

#[tokio::test]
async fn connections_come_back_in_the_canonical_envelope_with_the_key_header() {
    let (base, seen) = server(|_| {
        json(
            r#"{"items":[{"id":" ca_1 ","toolkit":"gmail","status":"ACTIVE"},
                         {"id":"  ","toolkit":"slack","status":"ACTIVE"}]}"#,
        )
    });
    let reply = list_connections(&credential(&base)).await.unwrap();
    assert_eq!(reply.connections.len(), 1, "a blank id is dropped");
    assert_eq!(reply.connections[0].id, "ca_1");

    let head = seen.lock().unwrap()[0].to_ascii_lowercase();
    assert!(
        head.starts_with("get /api/v3/connected_accounts?limit=200 "),
        "{head}"
    );
    assert!(head.contains("x-api-key: ck_test_key"), "{head}");
}

#[tokio::test]
async fn the_key_is_trimmed_before_it_is_sent() {
    let (base, seen) = server(|_| json(r#"{"items":[]}"#));
    let mut padded = credential(&base);
    padded.api_key = "  ck_padded \n".to_string();
    list_connections(&padded).await.unwrap();
    let head = seen.lock().unwrap()[0].to_ascii_lowercase();
    assert!(head.contains("x-api-key: ck_padded\r\n"), "{head}");
}

#[tokio::test]
async fn tools_send_repeated_tags_and_the_latest_toolkit_versions() {
    let (base, seen) = server(|_| {
        json(
            r#"{"items":[{"slug":"GITHUB_STAR","description":"Star","input_parameters":{"type":"object"},
                          "toolkit":{"slug":"github"}},{"slug":"","description":"junk"}]}"#,
        )
    });
    let reply = list_tools(
        &credential(&base),
        &["github".to_string()],
        &["stars".to_string(), "repos".to_string()],
    )
    .await
    .unwrap();
    assert_eq!(reply.tools.len(), 1);
    assert_eq!(reply.tools[0].function.name, "GITHUB_STAR");

    let head = seen.lock().unwrap()[0].clone();
    for part in [
        "limit=200",
        "toolkit_versions=latest",
        "toolkits=github",
        "tags=stars",
        "tags=repos",
    ] {
        assert!(head.contains(part), "{part} missing from {head}");
    }
}

#[tokio::test]
async fn a_rejected_key_reads_exactly_as_the_host_has_always_shown_it() {
    let (base, _) = server(|_| {
        (
            401,
            Vec::new(),
            r#"{"error":{"message":"Invalid API key"}}"#.to_string(),
        )
    });
    assert_eq!(
        list_connections(&credential(&base)).await.unwrap_err(),
        "Composio v3 connected_accounts failed: HTTP 401: Invalid API key"
    );
    assert_eq!(
        list_tools(&credential(&base), &[], &[]).await.unwrap_err(),
        "Composio v3 list_tool_schemas: HTTP 401: Invalid API key"
    );
}

#[tokio::test]
async fn a_failure_without_a_provider_message_is_just_the_status() {
    let (base, _) = server(|_| (500, Vec::new(), String::new()));
    assert_eq!(
        list_tools(&credential(&base), &[], &[]).await.unwrap_err(),
        "Composio v3 list_tool_schemas: HTTP 500"
    );
    let (base, _) = server(|_| (502, Vec::new(), "<html>bad gateway</html>".to_string()));
    assert_eq!(
        list_connections(&credential(&base)).await.unwrap_err(),
        "Composio v3 connected_accounts failed: HTTP 502"
    );
}

#[tokio::test]
async fn a_flat_message_is_used_and_identifiers_are_redacted_and_bounded() {
    let long = "x".repeat(400);
    let body = format!(r#"{{"message":"bad connected_account_id for user_id\nnext {long}"}}"#);
    let (base, _) = server(move |_| (400, Vec::new(), body.clone()));
    let error = list_connections(&credential(&base)).await.unwrap_err();
    assert!(
        error.starts_with("Composio v3 connected_accounts failed: HTTP 400: bad [redacted] for [redacted] next xxx"),
        "{error}"
    );
    assert!(error.ends_with("..."), "{error}");
    assert!(!error.contains('\n'));
    let message = error.trim_start_matches("Composio v3 connected_accounts failed: HTTP 400: ");
    assert_eq!(message.chars().count(), 240 + 3);
}

#[tokio::test]
async fn a_body_that_is_not_json_is_a_decode_failure_with_the_host_wording() {
    let (base, _) = server(|_| json("not json"));
    let error = list_connections(&credential(&base)).await.unwrap_err();
    assert!(
        error.starts_with(
            "Failed to decode Composio v3 connected_accounts response: error decoding response body: "
        ),
        "{error}"
    );
    let error = list_tools(&credential(&base), &[], &[]).await.unwrap_err();
    assert!(
        error.starts_with(
            "Failed to decode Composio v3 tools response: error decoding response body: "
        ),
        "{error}"
    );
}

#[tokio::test]
async fn a_dead_server_reads_as_a_failure_to_send_the_request() {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let error = list_connections(&credential(&format!("http://127.0.0.1:{port}/api/v3")))
        .await
        .unwrap_err();
    assert!(
        error.starts_with(&format!(
            "error sending request for url (http://127.0.0.1:{port}/api/v3/connected_accounts?limit=200)"
        )),
        "{error}"
    );
}

#[tokio::test]
async fn a_redirect_is_not_followed_and_never_carries_the_key() {
    let (destination, destination_seen) = server(|_| json(r#"{"items":[]}"#));
    let target = destination.clone();
    let (source, _) = server(move |_| {
        (
            302,
            vec![("Location", format!("{target}/elsewhere"))],
            String::new(),
        )
    });

    let error = list_connections(&credential(&source)).await.unwrap_err();
    assert_eq!(error, "Composio v3 connected_accounts failed: HTTP 302");
    assert!(
        destination_seen.lock().unwrap().is_empty(),
        "the redirect target must never be contacted"
    );
}

#[tokio::test]
async fn an_unsafe_base_url_is_refused_before_any_request() {
    for base in [
        "http://backend.composio.dev/api/v3",
        "http://127.0.0.1:1@evil.example/x",
    ] {
        let error = list_connections(&credential(base)).await.unwrap_err();
        assert!(
            error.starts_with("refusing to send a credential to "),
            "{error}"
        );
    }
}

#[tokio::test]
async fn an_empty_key_is_refused_without_a_request() {
    let mut empty = credential("http://127.0.0.1:1/api/v3");
    empty.api_key = "   ".to_string();
    assert_eq!(
        list_connections(&empty).await.unwrap_err(),
        "composio direct api key must not be empty"
    );
}

#[tokio::test]
async fn an_unusable_proxy_url_fails_the_call_without_echoing_it() {
    let (base, seen) = server(|_| json(r#"{"items":[]}"#));
    let mut bad = credential(&base);
    bad.transport = Some(ComposioTransportConfig {
        proxy_url: Some("ftp://user:hunter2@proxy.internal".to_string()),
        ..Default::default()
    });
    let error = list_connections(&bad).await.unwrap_err();
    assert!(error.starts_with("invalid network settings"), "{error}");
    assert!(!error.contains("hunter2"));
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn traffic_goes_through_the_hosts_proxy() {
    let (base, seen) =
        server(|_| json(r#"{"items":[{"id":"ca_1","toolkit":"gmail","status":"ACTIVE"}]}"#));
    let (proxy, proxied) = connect_proxy();
    let mut through = credential(&base);
    through.transport = Some(ComposioTransportConfig {
        proxy_url: Some(proxy),
        ..Default::default()
    });

    let reply = list_connections(&through).await.unwrap();
    assert_eq!(reply.connections[0].id, "ca_1");

    let connect = proxied.try_recv().expect("the proxy saw the request");
    let target = base
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap();
    assert!(
        connect.starts_with(&format!("CONNECT {target} ")),
        "{connect}"
    );
    assert_eq!(
        seen.lock().unwrap().len(),
        1,
        "and the server saw it through the proxy"
    );
}

#[tokio::test]
async fn a_destination_on_the_no_proxy_list_skips_the_proxy() {
    let (base, seen) = server(|_| json(r#"{"items":[]}"#));
    let (proxy, proxied) = connect_proxy();
    let mut direct = credential(&base);
    direct.transport = Some(ComposioTransportConfig {
        proxy_url: Some(proxy),
        no_proxy: vec!["127.0.0.1".to_string()],
        ..Default::default()
    });

    list_connections(&direct).await.unwrap();
    assert!(
        proxied.try_recv().is_err(),
        "a bypassed destination must not use the proxy"
    );
    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_proxy_that_refuses_the_tunnel_fails_the_call() {
    let (base, seen) = server(|_| json(r#"{"items":[]}"#));
    // Nothing listens on this port, so the proxy itself is unreachable.
    let dead = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let mut through = credential(&base);
    through.transport = Some(ComposioTransportConfig {
        proxy_url: Some(format!("http://127.0.0.1:{dead}")),
        ..Default::default()
    });
    let error = list_connections(&through).await.unwrap_err();
    assert!(
        error.starts_with("error sending request for url ("),
        "{error}"
    );
    assert!(
        seen.lock().unwrap().is_empty(),
        "the request must not bypass a configured proxy"
    );
}

#[tokio::test]
async fn the_suggested_fix_is_surfaced_redacted_and_bounded() {
    let body = r#"{"error":{"message":"Connected account user ID does not match","suggested_fix":"Use the user_id the account was created with"}}"#;
    let (base, _) = server(move |_| (400, Vec::new(), body.to_string()));
    assert_eq!(
        list_connections(&credential(&base)).await.unwrap_err(),
        "Composio v3 connected_accounts failed: HTTP 400: Connected account user ID does not match Suggested fix: Use the [redacted] the account was created with"
    );

    let long = "y".repeat(400);
    let body = format!(r#"{{"message":"bad","suggested_fix":"{long}"}}"#);
    let (base, _) = server(move |_| (400, Vec::new(), body.clone()));
    let error = list_connections(&credential(&base)).await.unwrap_err();
    assert!(
        error.contains("HTTP 400: bad Suggested fix: yyy"),
        "{error}"
    );
    assert!(error.ends_with("..."), "{error}");
}

#[tokio::test]
async fn a_blank_nested_fix_falls_back_to_the_top_level_one_within_the_bound() {
    let body = r#"{"error":{"message":"bad","suggested_fix":"  "},"suggested_fix":"try again"}"#;
    let (base, _) = server(move |_| (400, Vec::new(), body.to_string()));
    assert_eq!(
        list_connections(&credential(&base)).await.unwrap_err(),
        "Composio v3 connected_accounts failed: HTTP 400: bad Suggested fix: try again"
    );

    let long = "m".repeat(400);
    let body = format!(r#"{{"message":"{long}","suggested_fix":"{long}"}}"#);
    let (base, _) = server(move |_| (400, Vec::new(), body.clone()));
    let error = list_connections(&credential(&base)).await.unwrap_err();
    let detail = error.split("HTTP 400: ").nth(1).unwrap();
    assert!(
        detail.contains("Suggested fix: mmm"),
        "a long message must not crowd out the fix: {error}"
    );
    assert!(detail.chars().count() <= 240, "{error}");
}
