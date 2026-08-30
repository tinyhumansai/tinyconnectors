//! Unit tests for the execute pipeline.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;

use super::*;
use crate::client::route::Route;
use crate::{
    ComposioAuthorizeResponse, ComposioConnectionsResponse, ComposioDeleteResponse,
    ComposioExecuteResponse, ComposioToolkitsResponse, ComposioToolsResponse, Error, Result,
};

// ── prepare ─────────────────────────────────────────────────────────

#[test]
fn absent_arguments_become_an_empty_object() {
    // "no arguments" is a valid call for most actions, not a malformed one.
    for arguments in [None, Some(json!(null))] {
        let prepared = prepare_execute_arguments("GMAIL_FETCH_EMAILS", arguments).unwrap();
        assert_eq!(prepared, json!({}));
    }
}

#[test]
fn rejects_arguments_that_are_not_an_object() {
    let error = prepare_execute_arguments("ANY_TOOL", Some(json!(["a", "b"]))).unwrap_err();
    assert!(matches!(error, Error::InvalidArguments { .. }));
    assert!(error.to_string().contains("must be a JSON object"));
}

#[test]
fn promotes_a_bare_calendar_date_to_an_instant() {
    let prepared = prepare_execute_arguments(
        "GOOGLECALENDAR_FIND_EVENT",
        Some(json!({ "timeMin": "2026-05-14", "timeMax": "2026-05-15" })),
    )
    .unwrap();

    assert_eq!(prepared["timeMin"], "2026-05-14T00:00:00Z");
    assert_eq!(prepared["timeMax"], "2026-05-15T00:00:00Z");
}

#[test]
fn leaves_a_full_calendar_timestamp_alone() {
    let prepared = prepare_execute_arguments(
        "GOOGLECALENDAR_FIND_EVENT",
        Some(json!({ "time_min": "2026-05-14T09:30:00Z" })),
    )
    .unwrap();
    assert_eq!(prepared["time_min"], "2026-05-14T09:30:00Z");
}

#[test]
fn rejects_an_impossible_calendar_date_before_sending_it() {
    for bound in ["2026-99-99", "2026-02-30", "2025-02-29", "14-05-2026", "nonsense"] {
        let error = prepare_execute_arguments(
            "GOOGLECALENDAR_FIND_EVENT",
            Some(json!({ "timeMin": bound })),
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("RFC 3339"),
            "{bound} should be refused: {error}"
        );
    }
}

#[test]
fn accepts_a_leap_day_in_a_leap_year() {
    // The Gregorian rule, not just "divisible by four": 2024 and 2000 are leap
    // years, 1900 and 2025 are not.
    for (date, ok) in [
        ("2024-02-29", true),
        ("2000-02-29", true),
        ("1900-02-29", false),
        ("2025-02-29", false),
    ] {
        let result = prepare_execute_arguments(
            "GOOGLECALENDAR_FIND_EVENT",
            Some(json!({ "timeMin": date })),
        );
        assert_eq!(result.is_ok(), ok, "{date} leap-year handling");
    }
}

#[test]
fn accepts_every_month_length() {
    for (date, ok) in [
        ("2026-01-31", true),
        ("2026-04-30", true),
        ("2026-04-31", false),
        ("2026-12-31", true),
        ("2026-00-10", false),
        ("2026-13-01", false),
        ("2026-05-00", false),
    ] {
        let result = prepare_execute_arguments(
            "GOOGLECALENDAR_FIND_EVENT",
            Some(json!({ "timeMin": date })),
        );
        assert_eq!(result.is_ok(), ok, "{date} month-length handling");
    }
}

#[test]
fn infers_a_notion_fetch_type_rather_than_failing() {
    // Guessing wrong costs one unhelpful result; refusing costs the call.
    let prepared = prepare_execute_arguments("NOTION_FETCH_DATA", Some(json!({}))).unwrap();
    assert_eq!(prepared["fetch_type"], "pages");

    let prepared = prepare_execute_arguments(
        "NOTION_FETCH_DATA",
        Some(json!({ "filter": { "value": "database" } })),
    )
    .unwrap();
    assert_eq!(prepared["fetch_type"], "databases");
}

#[test]
fn keeps_a_notion_fetch_type_the_caller_supplied() {
    let prepared = prepare_execute_arguments(
        "NOTION_FETCH_DATA",
        Some(json!({ "fetch_type": "databases", "filter": { "value": "page" } })),
    )
    .unwrap();
    assert_eq!(prepared["fetch_type"], "databases");
}

#[test]
fn refuses_a_gmail_send_with_no_recipient() {
    let error =
        prepare_execute_arguments("GMAIL_SEND_EMAIL", Some(json!({ "subject": "hi" }))).unwrap_err();
    assert!(error.to_string().contains("recipient"));

    // An empty string is not a recipient either.
    let error =
        prepare_execute_arguments("GMAIL_SEND_EMAIL", Some(json!({ "to": "  " }))).unwrap_err();
    assert!(error.to_string().contains("recipient"));
}

#[test]
fn accepts_any_spelling_of_the_gmail_recipient_field() {
    for key in ["to", "recipient_email", "recipientEmail"] {
        assert!(
            prepare_execute_arguments("GMAIL_SEND_EMAIL", Some(json!({ key: "a@b.com" }))).is_ok(),
            "{key} should be accepted"
        );
    }
}

#[test]
fn refuses_a_label_change_that_would_do_nothing() {
    // Gmail treats a labelless call as a no-op success, so the agent would
    // report having labelled something it did not.
    let error = prepare_execute_arguments(
        "GMAIL_ADD_LABEL_TO_EMAIL",
        Some(json!({ "message_id": "m1", "add_label_ids": [] })),
    )
    .unwrap_err();
    assert!(error.to_string().contains("at least one non-empty label"));

    let error = prepare_execute_arguments(
        "GMAIL_ADD_LABEL_TO_EMAIL",
        Some(json!({ "add_label_ids": ["INBOX"] })),
    )
    .unwrap_err();
    assert!(error.to_string().contains("message_id"));
}

#[test]
fn accepts_a_label_change_that_only_removes() {
    assert!(
        prepare_execute_arguments(
            "GMAIL_ADD_LABEL_TO_EMAIL",
            Some(json!({ "messageId": "m1", "removeLabelIds": ["SPAM"] })),
        )
        .is_ok()
    );
}

// ── classify ────────────────────────────────────────────────────────

#[test]
fn a_404_wins_over_the_reauthenticate_text_it_carries() {
    // The whole reason ActionNotFound is checked first. A Composio 404 body
    // often literally reads "connection error, try to authenticate", and
    // telling the user to re-authenticate a healthy account over a stale
    // action name sends them down the wrong path entirely.
    let class = classify_composio_error(
        "GMAIL_SEND_EMAIL",
        "HTTP 404: connection error, try to authenticate",
    );
    assert_eq!(class, ComposioErrorClass::ActionNotFound);

    let rendered = format_provider_error("GMAIL_SEND_EMAIL", "HTTP 404: connection error");
    assert!(rendered.contains("still connected and working"));
    assert!(!rendered.contains("try to authenticate"));
}

#[test]
fn classifies_each_failure_shape() {
    for (message, expected) in [
        ("Missing required field `to`", ComposioErrorClass::Validation),
        (
            "Request had insufficient authentication scopes",
            ComposioErrorClass::InsufficientScope,
        ),
        (
            "403 Forbidden: you do not have permission to enable triggers on this connection",
            ComposioErrorClass::TriggerPermission,
        ),
        ("429 Too Many Requests", ComposioErrorClass::RateLimited),
        ("HTTP 410", ComposioErrorClass::ActionNotFound),
        (
            "connection error, try to authenticate",
            ComposioErrorClass::ComposioPlatform,
        ),
        ("Backend returned 502", ComposioErrorClass::Gateway),
    ] {
        assert_eq!(
            classify_composio_error("SLACK_SEND_MESSAGE", message),
            expected,
            "{message:?}"
        );
    }
}

#[test]
fn a_gateway_envelope_does_not_bury_the_provider_failure_inside_it() {
    // Classifying on the envelope would report "try again later" for a missing
    // scope, which no amount of trying again fixes.
    let class = classify_composio_error(
        "GMAIL_SEND_EMAIL",
        "Backend returned 502: insufficient authentication scopes",
    );
    assert_eq!(class, ComposioErrorClass::InsufficientScope);
}

#[test]
fn an_ordinary_403_is_not_a_trigger_permission_failure() {
    let class = classify_composio_error("GMAIL_SEND_EMAIL", "403 Forbidden");
    assert_ne!(class, ComposioErrorClass::TriggerPermission);
}

#[test]
fn only_the_transient_classes_are_worth_retrying() {
    assert!(ComposioErrorClass::RateLimited.is_transient());
    assert!(ComposioErrorClass::Gateway.is_transient());
    for class in [
        ComposioErrorClass::Validation,
        ComposioErrorClass::InsufficientScope,
        ComposioErrorClass::TriggerPermission,
        ComposioErrorClass::ActionNotFound,
        ComposioErrorClass::UpstreamProvider,
        ComposioErrorClass::ComposioPlatform,
        ComposioErrorClass::Other,
    ] {
        assert!(!class.is_transient(), "{class:?} must not invite a retry");
    }
}

#[test]
fn a_scope_failure_names_the_toolkit_and_the_fix() {
    let rendered = format_provider_error("GMAIL_SEND_EMAIL", "insufficient scope");
    assert!(rendered.starts_with("[composio:error:insufficient_scope]"));
    assert!(rendered.contains("Connections → gmail"));
}

// ── dispatch ────────────────────────────────────────────────────────

#[derive(Debug)]
struct ScriptedRoute {
    replies: Mutex<Vec<ComposioExecuteResponse>>,
    calls: Mutex<u32>,
}

impl ScriptedRoute {
    fn new(replies: Vec<ComposioExecuteResponse>) -> Arc<Self> {
        Arc::new(Self {
            replies: Mutex::new(replies),
            calls: Mutex::new(0),
        })
    }

    fn calls(&self) -> u32 {
        *self.calls.lock().unwrap()
    }
}

fn ok_response() -> ComposioExecuteResponse {
    ComposioExecuteResponse {
        successful: true,
        data: json!({ "ok": true }),
        ..ComposioExecuteResponse::default()
    }
}

fn failed_response(error: &str) -> ComposioExecuteResponse {
    ComposioExecuteResponse {
        successful: false,
        error: Some(error.to_string()),
        ..ComposioExecuteResponse::default()
    }
}

#[async_trait]
impl Route for ScriptedRoute {
    fn name(&self) -> &'static str {
        "scripted"
    }
    async fn list_toolkits(&self) -> Result<ComposioToolkitsResponse> {
        unimplemented!("not exercised by the execute tests")
    }
    async fn list_connections(&self) -> Result<ComposioConnectionsResponse> {
        unimplemented!("not exercised by the execute tests")
    }
    async fn authorize(&self, _: &str, _: &serde_json::Value) -> Result<ComposioAuthorizeResponse> {
        unimplemented!("not exercised by the execute tests")
    }
    async fn list_tools(&self, _: &[String], _: &[String]) -> Result<ComposioToolsResponse> {
        unimplemented!("not exercised by the execute tests")
    }
    async fn delete_connection(&self, _: &str) -> Result<ComposioDeleteResponse> {
        unimplemented!("not exercised by the execute tests")
    }

    async fn execute(
        &self,
        _tool: &str,
        _arguments: &serde_json::Value,
        _connection_id: Option<&str>,
    ) -> Result<ComposioExecuteResponse> {
        *self.calls.lock().unwrap() += 1;
        let mut replies = self.replies.lock().unwrap();
        if replies.len() > 1 {
            Ok(replies.remove(0))
        } else {
            Ok(replies[0].clone())
        }
    }
}

#[tokio::test(start_paused = true)]
async fn returns_a_successful_call_unchanged() {
    let route = ScriptedRoute::new(vec![ok_response()]);
    let response = execute_action(route.as_ref(), "GMAIL_FETCH_EMAILS", None, None)
        .await
        .unwrap();

    assert!(response.successful);
    assert!(response.error.is_none());
    assert_eq!(route.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn retries_once_while_a_fresh_connection_is_still_propagating() {
    let route = ScriptedRoute::new(vec![
        failed_response("Connection error, try to authenticate"),
        ok_response(),
    ]);
    let response = execute_action(route.as_ref(), "GMAIL_FETCH_EMAILS", None, None)
        .await
        .unwrap();

    assert!(response.successful);
    assert_eq!(route.calls(), 2);
}

#[tokio::test(start_paused = true)]
async fn does_not_retry_the_readiness_error_more_than_once() {
    // A revoked connection reports this forever. The user has to hear about it.
    let route = ScriptedRoute::new(vec![failed_response("Connection error, try to authenticate")]);
    let response = execute_action(route.as_ref(), "GMAIL_FETCH_EMAILS", None, None)
        .await
        .unwrap();

    assert!(!response.successful);
    assert_eq!(route.calls(), 2, "exactly one retry, then report");
}

#[tokio::test(start_paused = true)]
async fn a_reported_provider_failure_is_not_an_error() {
    // The call reached the provider and got a real answer. Only a call that
    // never completed is an `Err`.
    let route = ScriptedRoute::new(vec![failed_response("insufficient scope")]);
    let response = execute_action(route.as_ref(), "GMAIL_SEND_EMAIL", None, None)
        .await
        .unwrap();

    assert!(!response.successful);
    let error = response.error.unwrap();
    assert!(error.starts_with("[composio:error:insufficient_scope]"));
    assert!(error.contains("Reconnect"));
}

#[tokio::test(start_paused = true)]
async fn backs_off_through_a_rate_limit_on_the_allow_listed_action() {
    let mut replies = vec![failed_response("429 too many requests"); 3];
    replies.push(ok_response());
    let route = ScriptedRoute::new(replies);

    let response = execute_action(
        route.as_ref(),
        "SLACK_FETCH_CONVERSATION_HISTORY",
        None,
        None,
    )
    .await
    .unwrap();

    assert!(response.successful);
    assert_eq!(route.calls(), 4);
}

#[tokio::test(start_paused = true)]
async fn gives_up_on_a_rate_limit_after_the_attempt_limit() {
    let route = ScriptedRoute::new(vec![failed_response("429 too many requests")]);
    let response = execute_action(
        route.as_ref(),
        "SLACK_FETCH_CONVERSATION_HISTORY",
        None,
        None,
    )
    .await
    .unwrap();

    assert!(!response.successful);
    assert_eq!(route.calls(), RATE_LIMIT_MAX_ATTEMPTS);
    assert!(response.error.unwrap().contains("rate limit"));
}

#[tokio::test(start_paused = true)]
async fn surfaces_a_rate_limit_immediately_for_an_action_not_on_the_list() {
    // Stalling an agent turn for half a minute is worse than saying "slow down".
    let route = ScriptedRoute::new(vec![failed_response("429 too many requests")]);
    let response = execute_action(route.as_ref(), "GMAIL_FETCH_EMAILS", None, None)
        .await
        .unwrap();

    assert!(!response.successful);
    assert_eq!(route.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn does_not_call_out_when_local_validation_rejects_the_arguments() {
    let route = ScriptedRoute::new(vec![ok_response()]);
    let error = execute_action(
        route.as_ref(),
        "GMAIL_SEND_EMAIL",
        Some(json!({ "subject": "hi" })),
        None,
    )
    .await
    .unwrap_err();

    assert!(matches!(error, Error::InvalidArguments { .. }));
    assert_eq!(route.calls(), 0);
}
