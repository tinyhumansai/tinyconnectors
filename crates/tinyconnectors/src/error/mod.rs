//! Crate-wide error and result types.
//!
//! Every fallible public function in this crate returns [`Result`], and every
//! failure mode is a distinct [`Error`] variant. Add a variant rather than
//! encoding new context into an existing message: callers match on variants,
//! and message text is not a stable API.
//!
//! Variants carry the data a caller needs to react, keep their `#[error]`
//! message lowercase and free of trailing punctuation, and are documented so
//! the rendered rustdoc explains when each one occurs.

/// Errors returned by this crate.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// The connector backend rejected an OAuth authorize call.
    ///
    /// `message` is the upstream failure as the backend rendered it. It is kept
    /// verbatim because the backend proxies several providers and its message
    /// is usually the only thing that says which one refused, and why.
    #[error("authorize failed for {toolkit}: {message}")]
    Authorize {
        /// Toolkit slug the handoff was for.
        toolkit: String,
        /// Upstream failure, as rendered by the backend.
        message: String,
    },

    /// The transport failed, or the backend reported an error status.
    ///
    /// `path` is the backend-relative path so a failure names the call that
    /// produced it — several members hit the same host, and the message alone
    /// rarely says which one.
    #[error("request to {path} failed: {message}")]
    Transport {
        /// Backend-relative path the request targeted.
        path: String,
        /// Failure as the transport reported it.
        message: String,
    },

    /// The backend answered, but not in the shape this contract expects.
    ///
    /// Separate from [`Error::Transport`] because the two mean opposite things
    /// operationally: a transport failure is usually transient, while a decode
    /// failure means the backend changed and the contract has to catch up.
    #[error("response from {path} did not match the contract: {message}")]
    Decode {
        /// Backend-relative path whose response failed to decode.
        path: String,
        /// Deserialization failure.
        message: String,
    },

    /// Local validation rejected an action's arguments before the call.
    ///
    /// Caught here rather than at the provider because the provider's own
    /// rejection is often unhelpful — and sometimes absent: Gmail accepts a
    /// send with no recipient and a label change with no labels, then does
    /// nothing, which the caller reports as success.
    #[error("invalid arguments for `{tool}`: {message}")]
    InvalidArguments {
        /// Action slug the arguments were for.
        tool: String,
        /// Which rule rejected them.
        message: String,
    },

    /// The selected route does not offer this operation.
    ///
    /// The two routes are not equivalent, and pretending otherwise is worse
    /// than saying so: direct mode talks to Composio itself, where there is no
    /// per-user toolkit allowlist to list. A caller gets a named refusal it can
    /// act on rather than an empty result that looks like an answer.
    #[error("{member} is not available over the {route} route")]
    UnsupportedByRoute {
        /// Route that was asked, `"proxy"` or `"direct"`.
        route: &'static str,
        /// Bus member that was refused.
        member: &'static str,
    },

    /// A direct-mode API key was rejected repeatedly and is now gated.
    ///
    /// Without this, a revoked key makes every poll hit Composio and fail
    /// again, several times a minute, indefinitely. The gate stops asking until
    /// the user supplies a different key.
    #[error("{message}")]
    DirectAuthGated {
        /// User-facing explanation of the gate and how to clear it.
        message: String,
    },

    /// A base URL would send a credential somewhere it must not go.
    ///
    /// Refused before any request is made — see `client::HttpTransport`.
    #[error("refusing to send a credential to {base_url}: {reason}")]
    InsecureBaseUrl {
        /// The rejected base URL.
        base_url: String,
        /// Why it was rejected.
        reason: &'static str,
    },

    /// An OAuth host rate-limited the handoff and retries were exhausted.
    ///
    /// Distinct from [`Error::Authorize`] because it is not the user's request
    /// that was wrong — waiting fixes it — and because the message is guidance
    /// written for the user rather than an upstream string.
    #[error("{message}")]
    OauthRateLimited {
        /// Toolkit slug the handoff was for.
        toolkit: String,
        /// User-facing guidance explaining the wait and its likely cause.
        message: String,
    },
}

/// The crate's standard result type.
///
/// Use this alias in public signatures instead of spelling out
/// `std::result::Result<T, Error>`.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod test;
