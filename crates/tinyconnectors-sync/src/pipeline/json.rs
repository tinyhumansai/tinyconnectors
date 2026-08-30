//! Reading values out of a provider's JSON.
//!
//! Composio wraps provider payloads inconsistently — sometimes `data`,
//! sometimes `data.data`, sometimes neither — so every pipeline needs to try
//! several shapes for one field. These do that once.

use serde_json::Value;

/// The first non-empty scalar at any of `paths`.
///
/// Paths are dotted and resolve through JSON Pointer, so a numeric segment
/// indexes an array: `messages.0.id` is the first message's id.
///
/// Numbers are coerced to their string form, because provider ids are
/// inconsistently typed — the same field arrives as `"123"` from one endpoint
/// and `123` from another, and a caller building a record id cannot care which.
#[must_use]
pub fn pick_str(value: &Value, paths: &[&str]) -> Option<String> {
    paths.iter().find_map(|path| {
        let pointer = format!("/{}", path.replace('.', "/"));
        value
            .pointer(&pointer)
            .and_then(|found| match found {
                Value::String(text) => Some(text.clone()),
                Value::Number(number) => Some(number.to_string()),
                _ => None,
            })
            .map(|text| text.trim().to_owned())
            .filter(|text| !text.is_empty())
    })
}

/// The first array at any of `pointers`, or empty.
///
/// Pointers are JSON Pointer syntax (`/data/messages`), not dotted paths — the
/// callers of this are matching envelope shapes, where the leading slash is
/// what makes the nesting readable.
#[must_use]
pub fn first_array(value: &Value, pointers: &[&str]) -> Vec<Value> {
    pointers
        .iter()
        .find_map(|pointer| value.pointer(pointer).and_then(Value::as_array))
        .cloned()
        .unwrap_or_default()
}

/// A Google-style `nextPageToken` from any of the envelopes Composio uses.
///
/// Empty tokens are dropped rather than returned: an empty string is how
/// several providers say "no more pages", and treating it as a cursor makes the
/// next request ask for a page that does not exist — forever.
#[must_use]
pub fn next_page_token(value: &Value) -> Option<String> {
    [
        "/data/nextPageToken",
        "/nextPageToken",
        "/data/data/nextPageToken",
        "/data/next_page_token",
        "/next_page_token",
    ]
    .iter()
    .find_map(|pointer| value.pointer(pointer).and_then(Value::as_str))
    .map(str::trim)
    .filter(|token| !token.is_empty())
    .map(str::to_owned)
}
