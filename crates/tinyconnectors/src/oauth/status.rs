//! Classifying toolkits, connection statuses, and rate-limit failures.

/// Toolkits whose OAuth flows are hosted by Meta and share its rate limits.
pub const META_OAUTH_TOOLKITS: &[&str] = &["instagram", "facebook"];

/// Whether `toolkit` uses Meta-hosted OAuth.
///
/// Compared case-insensitively after trimming, because the slug reaches this
/// function from a UI field, a config file, and a backend envelope, and only
/// one of those three is reliably normalized.
#[must_use]
pub fn is_meta_oauth_toolkit(toolkit: &str) -> bool {
    let key = toolkit.trim().to_ascii_lowercase();
    META_OAUTH_TOOLKITS.contains(&key.as_str())
}

/// Whether a connection status means a handoff is still in flight.
///
/// These rows may still become active on their own — the user could be part-way
/// through the browser flow — so nothing should treat them as failures.
#[must_use]
pub fn is_inflight_oauth_status(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_uppercase().as_str(),
        "PENDING" | "INITIATED" | "INITIALIZING"
    )
}

/// Whether a non-active row is safe to delete before a fresh OAuth handoff.
///
/// This deliberately includes the in-flight statuses. Starting a new handoff is
/// the user saying "do this again", which supersedes whatever half-finished
/// attempt was open — and leaving those rows is precisely what trips Meta's
/// rate limiter.
#[must_use]
pub fn is_clearable_oauth_status(status: &str) -> bool {
    let upper = status.trim().to_ascii_uppercase();
    is_inflight_oauth_status(status) || matches!(upper.as_str(), "FAILED" | "ERROR" | "EXPIRED")
}

/// Whether a rendered authorize failure looks like upstream rate limiting.
///
/// Matched against text, not a status code: by the time a failure reaches this
/// module the backend proxy has already rendered the upstream response into a
/// message. The alternatives are all spellings observed in real failures.
#[must_use]
pub fn is_authorize_rate_limited(err: &str) -> bool {
    let lower = err.to_ascii_lowercase();
    lower.contains("429")
        || lower.contains("too many requests")
        || lower.contains("rate limit")
        || lower.contains("rate_limit")
        || lower.contains("ratelimited")
}

/// User-facing guidance when Meta OAuth is rate-limited.
///
/// The account hint is the useful half. A 429 here usually means the user is
/// retrying because the flow failed for a reason Meta reports poorly, and the
/// two common causes are account-type mistakes they can fix themselves.
#[must_use]
pub fn meta_oauth_rate_limit_message(toolkit: &str) -> String {
    let name = toolkit.trim();
    let account_hint = if name.eq_ignore_ascii_case("instagram") {
        " Use an Instagram Business or Creator account — personal accounts are not supported."
    } else if name.eq_ignore_ascii_case("facebook") {
        " Confirm the Facebook account has access to the relevant Page or Business Manager."
    } else {
        ""
    };
    format!(
        "Meta is temporarily rate-limiting {name} sign-in (HTTP 429). Wait a few \
         minutes before retrying and avoid clicking Connect repeatedly.{account_hint}"
    )
}
