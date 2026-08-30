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
