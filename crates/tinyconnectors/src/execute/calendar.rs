//! Default arguments for Google Calendar list and find actions.
//!
//! An agent (or a direct call site) that asks for calendar events builds the
//! argument object itself, and historically sent UTC-encoded `timeMin` and
//! `timeMax` with no `timeZone` and no `singleEvents`. For a user outside UTC
//! that goes wrong in two ways:
//!
//! 1. Without `singleEvents = true`, Google returns the parent recurring-event
//!    entry rather than the day's occurrence, and a filter keyed on the
//!    occurrence's `start.dateTime` never sees the user's recurring stand-up.
//! 2. Without `timeZone`, returned `start.dateTime` and `end.dateTime` are
//!    normalized against the *calendar's* default zone, which is not
//!    necessarily the requester's, so events stored against another zone slide
//!    outside the window.
//!
//! [`apply_calendar_query_defaults`] fills both in at the execute boundary so
//! neither the prompt template nor any call site needs per-call discipline.
//! Anything the caller supplied explicitly wins.
//!
//! The host's zone comes from the host, not from here: this crate is a
//! library and a module, and reading the machine's zone is the host's job.

use serde_json::Value;

/// Actions whose arguments accept, and benefit from, `timeZone` and
/// `singleEvents` defaulting.
///
/// Kept short on purpose: only actions that take a `timeMin`/`timeMax` window
/// belong here. A new action is worth adding when it accepts both fields and a
/// call site has been seen sending it without an explicit zone. Speculative
/// additions widen the transformer's reach for no current gain.
pub const TZ_DEFAULTING_SLUGS: &[&str] =
    &["GOOGLECALENDAR_EVENTS_LIST", "GOOGLECALENDAR_FIND_EVENT"];

/// Insert `singleEvents = true` and `timeZone = iana` into the arguments of a
/// calendar list or find action, where the caller left them out.
///
/// `arguments` of `None` means "no arguments supplied yet" and is treated as an
/// empty object so the defaults still apply. A payload that is not an object
/// passes through untouched, and so does any action outside
/// [`TZ_DEFAULTING_SLUGS`].
#[must_use]
pub fn apply_calendar_query_defaults(
    tool: &str,
    arguments: Option<Value>,
    iana: &str,
) -> Option<Value> {
    if !TZ_DEFAULTING_SLUGS.contains(&tool) {
        tracing::debug!(
            tool,
            "[connectors][calendar] action not in the tz-defaulting allowlist; pass-through"
        );
        return arguments;
    }

    let synthesised_object = arguments.is_none();
    let mut value = arguments.unwrap_or_else(|| Value::Object(serde_json::Map::default()));
    let Some(map) = value.as_object_mut() else {
        tracing::debug!(
            tool,
            "[connectors][calendar] non-object payload; pass-through unchanged"
        );
        return Some(value);
    };

    let injected_time_zone = !map.contains_key("timeZone");
    if injected_time_zone {
        map.insert("timeZone".to_string(), Value::String(iana.to_string()));
    }
    let injected_single_events = !map.contains_key("singleEvents");
    if injected_single_events {
        map.insert("singleEvents".to_string(), Value::Bool(true));
    }
    tracing::debug!(
        tool,
        iana,
        synthesised_object,
        injected_time_zone,
        injected_single_events,
        "[connectors][calendar] applied calendar query defaults"
    );
    Some(value)
}

#[cfg(test)]
#[path = "calendar_tests.rs"]
mod test;
