//! Recovering from a page the provider refused for its size.
//!
//! A page rejected purely because the response is too big is the one provider
//! failure a *smaller request* can fix. Halving and retrying costs one wasted
//! call; failing the run costs the sync — upstream, a single oversized page
//! left one workspace's mail unsynced for nine days.

/// The smallest page a shrink will ask for.
///
/// One item is the point past which a too-large response is about that one
/// item, not the batch size, and shrinking further cannot help.
pub const MIN_PAGE_SIZE: u64 = 1;

/// Whether the provider refused a page for its *size* rather than its content.
///
/// Matched on the error text because that is all the envelope carries: Composio
/// reports HTTP 413 with an `Upstream_PayloadTooLarge` slug, and other backends
/// phrase it as "payload too large" or "response too large".
#[must_use]
pub fn is_payload_too_large(error: Option<&str>) -> bool {
    error.is_some_and(|error| {
        let lower = error.to_ascii_lowercase();
        lower.contains("payloadtoolarge")
            || lower.contains("payload_too_large")
            || mentions_status_413(&lower)
            || (lower.contains("too large")
                && (lower.contains("payload") || lower.contains("response")))
    })
}

/// The next page size to try, or `None` when shrinking cannot help.
#[must_use]
pub fn shrink_page_size(current: u64) -> Option<u64> {
    if current <= MIN_PAGE_SIZE {
        return None;
    }
    Some((current / 2).max(MIN_PAGE_SIZE))
}

/// Whether `lower` names HTTP 413 as a status code.
///
/// The digits have to stand alone. An unanchored search also matches a message
/// id, an amount, or a timestamp containing those three digits, and every such
/// match costs a shrink-and-retry cycle before the real error surfaces — on a
/// failure a smaller page was never going to fix.
fn mentions_status_413(lower: &str) -> bool {
    lower.match_indices("413").any(|(at, _)| {
        let digit_before = lower[..at]
            .chars()
            .next_back()
            .is_some_and(|character| character.is_ascii_digit());
        let digit_after = lower[at + 3..]
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_digit());
        !digit_before && !digit_after
    })
}
