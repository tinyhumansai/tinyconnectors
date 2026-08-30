//! Deserializers that absorb upstream shape drift.
//!
//! Composio has repeatedly changed a previously-stringy field into an object
//! that carries the string plus render metadata — `"toolkit": "gmail"` becoming
//! `"toolkit": {"slug": "gmail", "logo": "…"}`. A plain `String` field rejects
//! the whole envelope when that happens, so the trigger listing goes empty and
//! a user's subscriptions look deleted.
//!
//! These two helpers accept both forms. They are private to [`super`]: they are
//! a tolerance for one backend's drift, not a vocabulary this crate publishes.

use serde::{Deserialize, Deserializer};

/// Accept either a JSON string or an object whose first matching field
/// (`slug` / `id` / `name` / `key`) is a string.
///
/// An object carrying none of those keys is an error rather than a silent
/// empty string: the field is required, and swallowing further drift here would
/// turn a visible decode failure into a trigger that quietly vanishes.
pub(super) fn de_string_or_object<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    use serde::de::Error;
    let v = serde_json::Value::deserialize(d)?;
    match v {
        serde_json::Value::String(s) => Ok(s),
        serde_json::Value::Object(map) => {
            for key in ["slug", "id", "name", "key"] {
                if let Some(serde_json::Value::String(s)) = map.get(key) {
                    return Ok(s.clone());
                }
            }
            Err(D::Error::custom(
                "expected string or object with slug/id/name/key field",
            ))
        }
        other => Err(D::Error::custom(format!(
            "expected string, got {}",
            match other {
                serde_json::Value::Null => "null",
                serde_json::Value::Bool(_) => "bool",
                serde_json::Value::Number(_) => "number",
                serde_json::Value::Array(_) => "array",
                _ => "unknown",
            }
        ))),
    }
}

/// Like [`de_string_or_object`] but optional and resilient.
///
/// Missing, null, and unrecognized object shapes all yield `None`. The key
/// order puts `state` and `value` first because the one optional field using
/// this — a trigger's state — arrives as `{"state": "ACTIVE", "slug": "…"}`,
/// where the `slug` describes the trigger and is not the state at all.
pub(super) fn de_opt_string_or_object<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Option<String>, D::Error> {
    let v = Option::<serde_json::Value>::deserialize(d)?;
    Ok(match v {
        Some(serde_json::Value::String(s)) => Some(s),
        Some(serde_json::Value::Object(map)) => {
            let mut found = None;
            for key in ["state", "value", "slug", "id", "name", "key"] {
                if let Some(serde_json::Value::String(s)) = map.get(key) {
                    found = Some(s.clone());
                    break;
                }
            }
            found
        }
        // Missing, null, and any other scalar all mean "not reported".
        _ => None,
    })
}
