//! Turning an action failure into something a user can act on.
//!
//! The problem this solves: Composio surfaces tool-level failures — a missing
//! OAuth scope, an unknown action, an upstream 429 — through the same channel
//! as genuine gateway trouble. Reported as "502, try again later", a missing
//! scope never gets fixed, because trying again never helps.
//!
//! Every message here is written for the person who has to do something about
//! it, and every class exists because it needs different advice.

/// What kind of failure an action produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ComposioErrorClass {
    /// The arguments were wrong. Fixed by calling differently.
    Validation,
    /// The connected account lacks a required OAuth scope. Fixed by
    /// reconnecting and granting it — never by retrying.
    InsufficientScope,
    /// The account may not manage triggers: a 403 whose body does *not* mention
    /// scopes, so [`Self::InsufficientScope`] cannot match it. Separate because
    /// the reconnect advice is the same but the explanation is not.
    TriggerPermission,
    /// An upstream rate limit. Fixed by waiting.
    RateLimited,
    /// Composio reported the action unknown or deprecated — an HTTP 404 or 410.
    ///
    /// Its own class because such a body frequently reads "connection error,
    /// try to authenticate", which would otherwise tell the user to
    /// re-authenticate an account that is working perfectly.
    ActionNotFound,
    /// The connected provider refused the call.
    UpstreamProvider,
    /// Composio's own platform reported a connection problem.
    ComposioPlatform,
    /// Transport-level trouble reaching the backend. The one class where
    /// "try again" is genuinely the right advice.
    Gateway,
    /// Nothing more specific matched.
    Other,
}

impl ComposioErrorClass {
    /// The stable, grep-friendly name used in the message prefix.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::InsufficientScope => "insufficient_scope",
            Self::TriggerPermission => "trigger_permission",
            Self::RateLimited => "rate_limited",
            Self::ActionNotFound => "action_not_found",
            Self::UpstreamProvider => "upstream_provider",
            Self::ComposioPlatform => "composio_platform",
            Self::Gateway => "gateway",
            Self::Other => "other",
        }
    }

    /// Whether retrying the same call unchanged could succeed.
    ///
    /// Only the two transient classes. A caller that retries on anything else
    /// is repeating a call that will fail identically.
    #[must_use]
    pub fn is_transient(self) -> bool {
        matches!(self, Self::RateLimited | Self::Gateway)
    }
}

/// Classify `message` as a failure of `tool`.
///
/// Order matters and is not arbitrary — see the comments at each arm.
#[must_use]
pub fn classify_composio_error(tool: &str, message: &str) -> ComposioErrorClass {
    let lower = message.to_ascii_lowercase();

    // First, because a 404 body frequently carries "connection error, try to
    // authenticate" — which the platform arm below would match, telling the
    // user to re-authenticate a healthy connection over a stale action name.
    let class = if is_action_not_found(&lower) {
        ComposioErrorClass::ActionNotFound
    } else if is_validation(&lower) {
        ComposioErrorClass::Validation
    } else if is_insufficient_scope(&lower) {
        ComposioErrorClass::InsufficientScope
    } else if is_trigger_permission(&lower) {
        ComposioErrorClass::TriggerPermission
    } else if is_rate_limited(&lower) {
        ComposioErrorClass::RateLimited
    } else if is_gateway(&lower) && !is_embedded_provider_failure(&lower) {
        // A gateway status wrapping a provider failure is the provider's
        // failure. Classifying on the envelope would bury it.
        ComposioErrorClass::Gateway
    } else if is_composio_platform(&lower) {
        ComposioErrorClass::ComposioPlatform
    } else if is_known_toolkit_action(tool) {
        ComposioErrorClass::UpstreamProvider
    } else {
        ComposioErrorClass::Other
    };

    tracing::debug!(
        tool = %tool,
        class = class.as_str(),
        "[connectors][classify] action failure classified"
    );
    class
}

/// Render `raw` as a message for the user, prefixed with its class.
#[must_use]
pub fn format_provider_error(tool: &str, raw: &str) -> String {
    let class = classify_composio_error(tool, raw);
    let detail = raw.trim();
    let body = match class {
        ComposioErrorClass::Validation => format!("Invalid arguments for `{tool}`: {detail}"),
        ComposioErrorClass::InsufficientScope => insufficient_scope_message(tool, detail),
        ComposioErrorClass::TriggerPermission => trigger_permission_message(tool),
        ComposioErrorClass::RateLimited => rate_limited_message(tool, detail),
        ComposioErrorClass::ActionNotFound => action_not_found_message(tool),
        ComposioErrorClass::UpstreamProvider => {
            format!("`{tool}` failed at the connected provider: {detail}")
        }
        ComposioErrorClass::ComposioPlatform => {
            format!("Composio connection issue for `{tool}`: {detail}")
        }
        ComposioErrorClass::Gateway => {
            format!("Temporary gateway error while calling `{tool}`: {detail}")
        }
        ComposioErrorClass::Other => format!("`{tool}` failed: {detail}"),
    };
    format!("[composio:error:{}] {}", class.as_str(), body)
}

/// The toolkit slug an action identifier belongs to.
///
/// Identifiers are upper-snake-case with the toolkit first, so
/// `GMAIL_NEW_GMAIL_MESSAGE` is `gmail`.
fn toolkit_of(tool: &str) -> String {
    tool.split('_')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn is_known_toolkit_action(tool: &str) -> bool {
    ["GMAIL_", "SLACK_", "NOTION_", "GOOGLECALENDAR_"]
        .iter()
        .any(|prefix| tool.starts_with(prefix))
}

fn insufficient_scope_message(tool: &str, detail: &str) -> String {
    let toolkit = toolkit_of(tool);
    format!(
        "`{tool}` was rejected because the connected {toolkit} account is missing required \
         permissions ({detail}). Reconnect the integration in Connections → {toolkit} and \
         grant the scopes requested during OAuth."
    )
}

fn trigger_permission_message(tool: &str) -> String {
    let toolkit = toolkit_of(tool);
    format!(
        "Couldn't enable this trigger: the connected {toolkit} account doesn't have permission \
         to manage triggers. Reconnect {toolkit} in Connections → {toolkit} and grant the \
         permissions requested during OAuth, then try again."
    )
}

/// Deliberately does not echo the raw detail.
///
/// A Composio 404 body often literally reads "connection error, try to
/// authenticate". Repeating that would reintroduce the misleading re-auth nudge
/// this class exists to suppress; the raw text is still logged for diagnosis.
fn action_not_found_message(tool: &str) -> String {
    let toolkit = toolkit_of(tool);
    format!(
        "`{tool}` couldn't run: Composio reported this action as not found. Your {toolkit} \
         integration is still connected and working — this is not a sign-in problem. The action \
         name is likely out of date or Composio's API changed; try again with the current action \
         name, or report this if it keeps happening."
    )
}

fn rate_limited_message(tool: &str, detail: &str) -> String {
    format!(
        "`{tool}` hit an upstream rate limit ({detail}). Wait a minute and retry, or reduce call \
         frequency — this is not a gateway outage."
    )
}

fn is_validation(lower: &str) -> bool {
    [
        "invalid arguments",
        "missing required",
        "must not be empty",
        "required field",
        "bad request",
        "invalid date",
        "rfc 3339",
        "timemax",
        "timemin",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn is_insufficient_scope(lower: &str) -> bool {
    lower.contains("insufficient authentication scopes")
        || lower.contains("insufficient scope")
        || lower.contains("insufficient permissions")
        || lower.contains("invalid oauth scope")
        || (lower.contains("403") && lower.contains("scope"))
}

/// A trigger-permission denial.
///
/// The 403 body reads "You do not have permission to enable triggers on this
/// connection" — note it carries no "scope" token, so the scope check above
/// never matches it. All three signals are required so an ordinary 403 is not
/// swept in.
fn is_trigger_permission(lower: &str) -> bool {
    let forbidden = lower.contains("403") || lower.contains("forbidden");
    let denied = [
        "do not have permission",
        "not have permission",
        "permission denied",
        "not permitted",
        "not allowed",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
    forbidden && denied && lower.contains("trigger")
}

fn is_rate_limited(lower: &str) -> bool {
    [
        "rate limit",
        "rate_limit",
        "ratelimited",
        "too many requests",
        "429",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

/// Scoped to 404 and 410 on purpose: unknown action, or deprecated endpoint.
/// Other statuses keep their validation, scope, rate-limit, or gateway class.
fn is_action_not_found(lower: &str) -> bool {
    lower.contains("http 404") || lower.contains("http 410")
}

fn is_composio_platform(lower: &str) -> bool {
    [
        "connection error, try to authenticate",
        "not enabled",
        "not connected",
        "token revoked",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn is_gateway(lower: &str) -> bool {
    ["502", "503", "504"].iter().any(|code| {
        lower.contains(&format!("backend returned {code}")) || lower.contains(&format!("({code} "))
    }) || lower.contains("502 bad gateway")
}

/// Whether a message names a provider-level failure, even inside a gateway
/// envelope. Used to stop a wrapping 502 from masking the real cause.
fn is_embedded_provider_failure(lower: &str) -> bool {
    is_validation(lower)
        || is_insufficient_scope(lower)
        || is_trigger_permission(lower)
        || is_rate_limited(lower)
        || is_action_not_found(lower)
        || is_composio_platform(lower)
        || [
            "composio",
            "google",
            "slack",
            "notion",
            "gmail",
            "fetch_type",
        ]
        .iter()
        .any(|needle| lower.contains(needle))
}
