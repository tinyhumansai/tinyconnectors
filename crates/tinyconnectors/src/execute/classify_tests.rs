//! Additional classification and rendering tests, ported from the host's
//! former copy of this logic.

use super::{ComposioErrorClass, classify_composio_error, format_provider_error};

#[test]
fn a_real_platform_failure_without_an_http_status_stays_platform() {
    assert_eq!(
        classify_composio_error("GMAIL_SEND_EMAIL", "connection error, try to authenticate"),
        ComposioErrorClass::ComposioPlatform
    );
}

#[test]
fn a_gone_endpoint_is_an_unknown_action_not_a_broken_connection() {
    assert_eq!(
        classify_composio_error(
            "GOOGLEDOCS_UPDATE_DOCUMENT",
            "HTTP 410: This endpoint is deprecated"
        ),
        ComposioErrorClass::ActionNotFound
    );
}

#[test]
fn an_unknown_action_message_does_not_send_the_user_to_reconnect() {
    let rendered = format_provider_error(
        "GMAIL_SEND_EMAIL",
        "HTTP 404: connection error, try to authenticate",
    );
    let lower = rendered.to_lowercase();
    assert!(rendered.contains("[composio:error:action_not_found]"));
    assert!(lower.contains("still connected"));
    assert!(!lower.contains("authenticate"), "{rendered}");
    assert!(!lower.contains("reconnect"), "{rendered}");
    assert!(!rendered.contains("Connections →"), "{rendered}");
}

#[test]
fn the_wrapped_v3_then_v2_fallback_string_is_an_unknown_action() {
    let raw = "Composio execute failed on v3 (Composio v3 action execution failed: \
               HTTP 404: connection error, try to authenticate) and v2 fallback \
               (Composio v2 action execution failed: HTTP 410: Gone)";
    let rendered = format_provider_error("GMAIL_SEND_EMAIL", raw);
    assert!(
        rendered.contains("[composio:error:action_not_found]"),
        "{rendered}"
    );
    assert!(
        !rendered.to_lowercase().contains("authenticate"),
        "{rendered}"
    );
}

#[test]
fn a_trigger_permission_403_is_not_a_scope_failure() {
    let raw = "Backend returned 403 Forbidden for POST \
               https://api.example.com/agent-integrations/composio/triggers: \
               You do not have permission to enable triggers on this connection";
    assert_eq!(
        classify_composio_error("GMAIL_NEW_GMAIL_MESSAGE", raw),
        ComposioErrorClass::TriggerPermission
    );
}

#[test]
fn trigger_permission_guidance_names_the_toolkit_and_hides_the_raw_body() {
    let raw = "Backend returned 403 Forbidden for POST \
               https://api.example.com/agent-integrations/composio/triggers: \
               You do not have permission to enable triggers on this connection";
    let rendered = format_provider_error("GMAIL_NEW_GMAIL_MESSAGE", raw);
    assert!(
        rendered.contains("[composio:error:trigger_permission]"),
        "{rendered}"
    );
    assert!(rendered.contains("Connections → gmail"), "{rendered}");
    assert!(rendered.to_lowercase().contains("permission"), "{rendered}");
    assert!(!rendered.contains("Backend returned 403"), "{rendered}");
}

#[test]
fn a_true_gateway_failure_stays_a_gateway_failure() {
    let rendered = format_provider_error("CUSTOM_ACTION", "Backend returned 503: upstream down");
    assert!(rendered.contains("[composio:error:gateway]"), "{rendered}");
    assert!(
        rendered.contains("Temporary gateway error while calling `CUSTOM_ACTION`"),
        "{rendered}"
    );
}

#[test]
fn a_rate_limit_message_states_the_detail_and_the_advice() {
    let rendered = format_provider_error("SLACK_FETCH_CONVERSATION_HISTORY", "429");
    assert!(
        rendered.contains("[composio:error:rate_limited]"),
        "{rendered}"
    );
    assert!(
        rendered.contains("hit an upstream rate limit (429)"),
        "{rendered}"
    );
    assert!(rendered.contains("Wait a minute and retry"), "{rendered}");
}

#[test]
fn an_empty_action_identifier_yields_an_empty_toolkit_slug() {
    let rendered = format_provider_error("", "insufficient scope");
    assert!(rendered.contains("Connections → "), "{rendered}");
}
