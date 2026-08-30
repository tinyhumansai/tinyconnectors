//! Reading an identity field out of a provider's payload.

/// The first non-empty string at any of `paths`.
///
/// Each path is dot-separated, so a nested `person.email` is reachable without
/// every provider hand-writing the same walk. Paths are tried in order, which
/// is how a provider expresses "prefer the display name, fall back to the
/// login" in one call.
///
/// Returns `None` rather than an empty string: "the provider did not report
/// this" and "the provider reported an empty one" are the same thing to a
/// caller picking a label, and an empty string would render as a blank account.
pub(super) fn pick(payload: &serde_json::Value, paths: &[&str]) -> Option<String> {
    paths.iter().find_map(|path| {
        let mut current = payload;
        for segment in path.split('.') {
            current = current.get(segment)?;
        }
        current
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

#[cfg(test)]
#[path = "identity_test.rs"]
mod test;
