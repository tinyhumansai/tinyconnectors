
// ── the backend envelope ─────────────────────────────────────────────

#[tokio::test]
async fn the_proxy_route_reads_the_payload_out_of_the_backend_envelope() {
    // The failure this prevents is silent: the contract's response types
    // default their fields, so decoding the wrapper as the payload answers
    // with an empty list on a reply that carried a full one. A user sees "no
    // toolkits enabled" and has no reason to think anything broke.
    let transport = Arc::new(StubTransport::replying(json!({
        "success": true,
        "data": { "toolkits": ["gmail", "slack"] },
    })));
    let route = ProxyRoute::new(transport);

    let response = route.list_toolkits().await.expect("decodes");
    assert_eq!(response.toolkits, vec!["gmail", "slack"]);
}

#[tokio::test]
async fn an_unwrapped_reply_is_still_decoded_as_the_payload() {
    // Not every endpoint on the far side wraps. Refusing an unwrapped reply
    // would turn a working endpoint into a decode error over a difference that
    // carries no meaning.
    let transport = Arc::new(StubTransport::replying(json!({
        "toolkits": ["gmail"],
    })));
    let route = ProxyRoute::new(transport);

    let response = route.list_toolkits().await.expect("decodes");
    assert_eq!(response.toolkits, vec!["gmail"]);
}

#[tokio::test]
async fn a_failed_envelope_is_an_error_carrying_what_the_backend_said() {
    // `success: false` is how the backend reports the things a user needs
    // told — a toolkit that is not enabled, a missing required field. Decoded
    // as a payload it would become an empty result, and the user would never
    // learn why.
    let transport = Arc::new(StubTransport::replying(json!({
        "success": false,
        "error": "Toolkit `notion` is not enabled for this account",
    })));
    let route = ProxyRoute::new(transport);

    let error = route.list_toolkits().await.unwrap_err().to_string();
    assert!(error.contains("notion"), "{error}");
    assert!(error.contains("not enabled"), "{error}");
}

#[tokio::test]
async fn a_successful_envelope_with_no_data_is_an_error_rather_than_an_empty_answer() {
    let transport = Arc::new(StubTransport::replying(json!({ "success": true })));
    let route = ProxyRoute::new(transport);

    let error = route.list_toolkits().await.unwrap_err().to_string();
    assert!(error.contains("no data"), "{error}");
}
