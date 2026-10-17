//! How a preference value is stored as a document body.
//!
//! A document body must be a JSON object, but a preference is any JSON value.
//! An object value becomes the body as it is. Any other value is stored under
//! [`NON_OBJECT_FIELD`]: `{ "non_object_value": <value> }`. An object value that
//! itself uses that field is wrapped the same way, so a body with exactly that
//! one field always means "wrapped" and the encoding loses nothing.

use serde_json::{Value, json};

use super::names::NON_OBJECT_FIELD;

/// The v2 body for a preference value.
#[must_use]
pub fn encode_preference(value: &Value) -> Value {
    match value {
        Value::Object(fields) if !fields.contains_key(NON_OBJECT_FIELD) => value.clone(),
        other => json!({ NON_OBJECT_FIELD: other }),
    }
}

/// The preference value a v2 body holds: the inverse of [`encode_preference`].
#[must_use]
pub fn decode_preference(body: Value) -> Value {
    match body {
        Value::Object(mut fields) if fields.len() == 1 && fields.contains_key(NON_OBJECT_FIELD) => {
            fields.remove(NON_OBJECT_FIELD).unwrap_or(Value::Null)
        }
        other => other,
    }
}
