//! OAuth handoff policy: which statuses mean what, and how to survive Meta's
//! rate limiter.
//!
//! Linking an account is a handoff, not a call. The module asks the backend to
//! start one, gets back a hosted URL, and the user finishes it in a browser
//! minutes later — or abandons it. Everything awkward about this module follows
//! from that gap.
//!
//! # Abandoned handoffs accumulate
//!
//! Each authorize call creates a connection row immediately, in a non-active
//! state. A user who clicks Connect, hesitates, and clicks again leaves two.
//! [`is_clearable_oauth_status`] identifies the rows that a fresh handoff may
//! delete, and [`is_inflight_oauth_status`] separates "still might succeed"
//! from "already failed" within that set.
//!
//! # Meta rate-limits the accumulation
//!
//! Instagram and Facebook share one OAuth host that returns HTTP 429 when too
//! many sessions are created in a short window — which is exactly what the
//! retry-then-retry pattern above produces. Two mitigations live here:
//! clearing stale rows before starting a new handoff (the caller does this with
//! [`is_meta_oauth_toolkit`] and [`is_clearable_oauth_status`]), and backing off
//! when a 429 does arrive ([`authorize_with_rate_limit_retry`]).
//!
//! The 429 is detected by [`is_authorize_rate_limited`] against the *rendered*
//! error text rather than a status code. That is deliberate: the failure
//! reaches this module through a backend proxy that has already turned the
//! upstream response into a message, so the code is gone by the time we see it.
//!
//! # Why the guidance message is here and not in the UI
//!
//! [`meta_oauth_rate_limit_message`] returns prose, which looks out of place in
//! a policy module. A rate-limited Meta sign-in is almost always one of two
//! specific, fixable mistakes — a personal Instagram account rather than a
//! Business one, or a Facebook account without Page access — and the toolkit
//! slug is the only thing that distinguishes them. Rendering it here keeps
//! every caller telling the user the same true thing.

mod retry;
mod status;

pub use retry::{
    AUTHORIZE_RATE_LIMIT_MAX_ATTEMPTS, authorize_with_rate_limit_retry,
    wrap_authorize_rate_limit_error,
};
pub use status::{
    META_OAUTH_TOOLKITS, is_authorize_rate_limited, is_clearable_oauth_status,
    is_inflight_oauth_status, is_meta_oauth_toolkit, meta_oauth_rate_limit_message,
};

#[cfg(test)]
mod test;
