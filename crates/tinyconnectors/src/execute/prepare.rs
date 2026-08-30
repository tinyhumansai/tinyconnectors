//! Normalizing and validating action arguments before they are sent.
//!
//! Every rule here encodes a failure that is cheap to catch locally and
//! expensive to catch remotely — either because the provider's error is
//! unhelpful, or because the call half-succeeds.

use serde_json::{Map, Value};

use crate::{Error, Result};

/// Normalize and validate `arguments` for `tool`.
///
/// Absent arguments become an empty object, because most actions accept one and
/// a caller omitting them means "no arguments", not "invalid".
///
/// # Errors
///
/// Returns [`Error::InvalidArguments`] when the arguments are not an object, or
/// when a per-action rule rejects them.
pub fn prepare_execute_arguments(tool: &str, arguments: Option<Value>) -> Result<Value> {
    let tool = tool.trim();
    let mut args = match arguments {
        Some(Value::Object(map)) => Value::Object(map),
        Some(Value::Null) | None => Value::Object(Map::new()),
        Some(other) => {
            return Err(invalid(
                tool,
                format!("arguments must be a JSON object, got {other}"),
            ));
        }
    };

    if tool.starts_with("GOOGLECALENDAR_") {
        normalize_calendar_time_bounds(tool, &mut args)?;
    }
    match tool {
        "NOTION_FETCH_DATA" => ensure_notion_fetch_type(&mut args),
        "GMAIL_SEND_EMAIL" => validate_gmail_send_email(tool, &args)?,
        "GMAIL_ADD_LABEL_TO_EMAIL" => validate_gmail_add_label(tool, &args)?,
        _ => {}
    }

    Ok(args)
}

fn invalid(tool: &str, message: impl Into<String>) -> Error {
    Error::InvalidArguments {
        tool: tool.to_string(),
        message: message.into(),
    }
}

/// Promote a bare date to an RFC 3339 instant, and reject one that is not a date.
///
/// Google Calendar rejects `2026-05-14` where it wants a timestamp, with an
/// error that does not say so. Promoting it to midnight UTC is what the caller
/// meant. An impossible date like `2026-99-99` is refused here rather than
/// forwarded, so the user hears which argument was wrong.
fn normalize_calendar_time_bounds(tool: &str, args: &mut Value) -> Result<()> {
    let Some(object) = args.as_object_mut() else {
        return Ok(());
    };
    for key in ["timeMin", "timeMax", "time_min", "time_max"] {
        let Some(value) = object.get(key).cloned() else {
            continue;
        };
        if let Some(normalized) = normalize_rfc3339_bound(&value) {
            object.insert(key.to_string(), Value::String(normalized));
        } else if value.is_string() {
            return Err(invalid(
                tool,
                format!(
                    "time bound `{key}` must be an RFC 3339 timestamp \
                     (e.g. 2026-05-14T00:00:00Z), not a bare date"
                ),
            ));
        }
    }
    Ok(())
}

fn normalize_rfc3339_bound(value: &Value) -> Option<String> {
    let text = value.as_str()?.trim();
    if text.is_empty() {
        return None;
    }
    if text.contains('T') {
        return Some(text.to_string());
    }
    is_calendar_date(text).then(|| format!("{text}T00:00:00Z"))
}

/// Whether `text` is a real `YYYY-MM-DD` date.
///
/// Hand-checked rather than parsed with a date library: this is the only date
/// handling in the crate, and the rule is small enough to state exactly. The
/// month lengths and the Gregorian leap rule are covered by tests.
fn is_calendar_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let (Ok(year), Ok(month), Ok(day)) = (
        text[0..4].parse::<u32>(),
        text[5..7].parse::<u32>(),
        text[8..10].parse::<u32>(),
    ) else {
        return false;
    };
    if !(1..=12).contains(&month) || day == 0 {
        return false;
    }
    day <= days_in_month(year, month)
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Supply the `fetch_type` Notion requires and callers routinely omit.
///
/// Inferred from the filter when possible, defaulting to pages — which is what
/// a caller who did not think about it almost always wants. Never an error:
/// guessing wrong costs one unhelpful result, while refusing costs the call.
fn ensure_notion_fetch_type(args: &mut Value) {
    let Some(object) = args.as_object_mut() else {
        return;
    };
    let already_set = object
        .get("fetch_type")
        .or_else(|| object.get("fetchType"))
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());
    if already_set {
        return;
    }

    let inferred = object
        .get("filter")
        .and_then(|filter| filter.get("value").or_else(|| filter.get("property")))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| match value {
            "page" | "pages" => "pages",
            "database" | "databases" => "databases",
            other => other,
        })
        .unwrap_or("pages");

    tracing::debug!(
        fetch_type = %inferred,
        "[connectors][prepare] NOTION_FETCH_DATA: inferred fetch_type"
    );
    object.insert(
        "fetch_type".to_string(),
        Value::String(inferred.to_string()),
    );
}

/// A send with no recipient is refused locally.
///
/// Gmail accepts the call and the mail goes nowhere, which the user discovers
/// only by its absence.
fn validate_gmail_send_email(tool: &str, args: &Value) -> Result<()> {
    let object = args
        .as_object()
        .ok_or_else(|| invalid(tool, "arguments must be an object"))?;
    let has_recipient = ["to", "recipient_email", "recipientEmail"]
        .iter()
        .filter_map(|key| object.get(*key))
        .filter_map(Value::as_str)
        .any(|value| !value.trim().is_empty());

    if has_recipient {
        Ok(())
    } else {
        Err(invalid(
            tool,
            "`to` (or `recipient_email`) is required — cannot send without a recipient",
        ))
    }
}

/// A label change needs a message and at least one label to change.
///
/// Gmail treats a labelless call as a no-op success, so without this the agent
/// reports having labelled something it did not.
fn validate_gmail_add_label(tool: &str, args: &Value) -> Result<()> {
    let object = args
        .as_object()
        .ok_or_else(|| invalid(tool, "arguments must be an object"))?;

    let has_message = ["message_id", "messageId"]
        .iter()
        .filter_map(|key| object.get(*key))
        .filter_map(Value::as_str)
        .any(|value| !value.trim().is_empty());
    if !has_message {
        return Err(invalid(tool, "`message_id` is required"));
    }

    let changes_a_label = [
        "add_label_ids",
        "addLabelIds",
        "remove_label_ids",
        "removeLabelIds",
    ]
    .iter()
    .any(|key| has_non_empty_string(object.get(*key)));
    if changes_a_label {
        Ok(())
    } else {
        Err(invalid(
            tool,
            "provide at least one non-empty label in `add_label_ids` or `remove_label_ids`",
        ))
    }
}

fn has_non_empty_string(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .any(|item| !item.trim().is_empty()),
        Some(Value::String(text)) => !text.trim().is_empty(),
        _ => false,
    }
}
