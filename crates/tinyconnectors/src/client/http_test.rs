//! Unit tests for the HTTP transport's base-URL guard.
//!
//! These are the tests that matter most in this file: everything else here
//! sends a request, but this decides whether a user's credential is allowed to
//! leave the machine at all.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc;

use super::{HttpTransport, check_base_url};
use crate::Error;
use crate::client::Transport;

/// A one-request HTTP server on loopback.
///
/// The guard allows plain HTTP to a genuine loopback address precisely so this
/// is possible: the transport's actual request — its method, path, and headers
/// — is otherwise only exercised against a live backend.
///
/// Returns the base URL and a channel carrying the request line and headers the
/// transport actually sent.
fn loopback_server(response_body: &'static str) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("binds a port");
    let port = listener.local_addr().expect("has an address").port();
    let (sender, receiver) = mpsc::channel();

    std::thread::spawn(move || {
        let Ok((stream, _)) = listener.accept() else {
            return;
        };
        let mut reader = BufReader::new(&stream);
        let mut request = String::new();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                break;
            }
            request.push_str(&line);
        }
        let _ = sender.send(request);

        let mut stream = &stream;
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        let _ = stream.flush();
    });

    (format!("http://127.0.0.1:{port}"), receiver)
}

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

#[tokio::test]
async fn a_get_sends_the_bearer_header_and_decodes_the_body() {
    let (base_url, requests) = loopback_server(r#"{"toolkits":["gmail"]}"#);
    let transport = HttpTransport::bearer(&base_url, "t0ken").unwrap();

    let value = transport
        .get("/agent-integrations/composio/toolkits")
        .await
        .unwrap();
    assert_eq!(value["toolkits"][0], "gmail");

    let request = requests.recv().expect("the server saw a request");
    assert!(
        request.starts_with("GET /agent-integrations/composio/toolkits"),
        "{request}"
    );
    assert!(
        request
            .to_lowercase()
            .contains("authorization: bearer t0ken"),
        "{request}"
    );
}

#[tokio::test]
async fn a_post_sends_the_body_and_the_api_key_header() {
    let (base_url, requests) = loopback_server(r#"{"successful":true}"#);
    let transport = HttpTransport::api_key(&base_url, "sk-1").unwrap();

    let value = transport
        .post(
            "/tools/execute/GMAIL_SEND_EMAIL",
            &serde_json::json!({ "a": 1 }),
        )
        .await
        .unwrap();
    assert_eq!(value["successful"], true);

    let request = requests.recv().expect("the server saw a request");
    assert!(
        request.starts_with("POST /tools/execute/GMAIL_SEND_EMAIL"),
        "{request}"
    );
    assert!(
        request.to_lowercase().contains("x-api-key: sk-1"),
        "{request}"
    );
    assert!(
        !request.to_lowercase().contains("authorization:"),
        "the direct route must not also send a bearer token: {request}"
    );
}

#[tokio::test]
async fn a_delete_reaches_the_item_path() {
    let (base_url, requests) = loopback_server(r#"{"deleted":true}"#);
    let transport = HttpTransport::bearer(&base_url, "t0ken").unwrap();

    let value = transport
        .delete("/agent-integrations/composio/connections/conn_9")
        .await
        .unwrap();
    assert_eq!(value["deleted"], true);

    let request = requests.recv().expect("the server saw a request");
    assert!(
        request.starts_with("DELETE /agent-integrations/composio/connections/conn_9"),
        "{request}"
    );
}

#[tokio::test]
async fn a_body_that_is_not_json_is_a_decode_failure_naming_the_path() {
    // Distinguishable from a transport failure, because retrying cannot fix it.
    let (base_url, _requests) = loopback_server("not json at all");
    let transport = HttpTransport::bearer(&base_url, "t0ken").unwrap();

    let error = transport
        .get("/agent-integrations/composio/toolkits")
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Decode { .. }));
    assert!(
        error
            .to_string()
            .contains("/agent-integrations/composio/toolkits")
    );
}

#[tokio::test]
async fn a_refused_connection_is_a_transport_failure_naming_the_path() {
    // Bind and drop, so the port is closed but plausible.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let transport = HttpTransport::bearer(&format!("http://127.0.0.1:{port}"), "t").unwrap();
    let error = transport.get("/anything").await.unwrap_err();

    assert!(matches!(error, Error::Transport { .. }));
    assert!(error.to_string().contains("/anything"));
}
