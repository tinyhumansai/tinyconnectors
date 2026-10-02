//! Running one action against a connected account.
//!
//! Executing is the only member that changes something in a user's account, and
//! almost everything here exists because of a way that goes wrong in practice.
//!
//! # The pipeline
//!
//! 1. [`prepare_execute_arguments`] normalizes and validates the arguments
//!    locally. A bare date
//!    that Google Calendar would reject, a Gmail send with no recipient —
//!    these are caught before the call rather than after it.
//! 2. The call goes out through the live [`crate::client::route::Route`], with
//!    the two retry policies below.
//! 3. [`classify_composio_error`] and [`format_provider_error`] turn whatever
//!    came back into a class and a message a user can act on.
//!
//! # Two retries, and why not three
//!
//! **Post-OAuth readiness.** Composio reports a connection `ACTIVE` a second or
//! two after the user finishes OAuth, but its execution gateway can take
//! another half-minute to sync the token. In that window every call returns the
//! literal `"connection error, try to authenticate"` for a connection that is
//! genuinely fine. One retry after a short sleep clears it. It is deliberately
//! one: a revoked connection produces the same string forever, and the user
//! should hear about it after one round-trip rather than never.
//!
//! **Upstream rate limits.** Only for [`RATE_LIMIT_RETRY_TOOLS`]. Slack's
//! conversation history is the one action bursty agent reads reliably trip a
//! 429 on, and it has stable retry semantics. Everything else surfaces the 429
//! to the caller, because silently stalling an agent turn for half a minute is
//! worse than telling it to slow down.
//!
//! The upstream this was ported from had three retry layers that could stack —
//! an in-client retry, a wrapper around a non-retrying primitive that existed
//! only to avoid the first, and this rate-limit loop — which its own comments
//! record as issuing up to four calls per logical retry. Here the policy lives
//! in exactly one place.
//!
//! # Optional argument and response shaping
//!
//! Two helpers are not part of [`execute_action`] and a host applies them
//! around the call when it wants them: [`apply_calendar_query_defaults`]
//! (a `timeZone` and `singleEvents` default for calendar list actions, given
//! the zone by the host) and [`apply_window_args`] / [`filter_response`]
//! (a recency window for task-fetch actions, given the `since` instant by the
//! host).
//!
//! # What is deliberately not here
//!
//! **Egress enforcement.** `OpenHuman` refuses outbound tool calls under its
//! local-only privacy mode, and emits a disclosure for every external transfer.
//! That is host policy about the user's data, and the host applies it before
//! calling this member — a module that enforced it would be trusting a
//! policy decision it cannot see the reasons for.

mod calendar;
mod classify;
mod prepare;
mod retry;
mod task_window;

pub use calendar::{TZ_DEFAULTING_SLUGS, apply_calendar_query_defaults};
pub use classify::{ComposioErrorClass, classify_composio_error, format_provider_error};
pub use prepare::prepare_execute_arguments;
pub use retry::{
    POST_OAUTH_RETRY_DELAY, RATE_LIMIT_MAX_ATTEMPTS, RATE_LIMIT_RETRY_TOOLS, execute_action,
};
pub use task_window::{apply_window_args, filter_response};

#[cfg(test)]
#[path = "mod_tests.rs"]
mod test;
