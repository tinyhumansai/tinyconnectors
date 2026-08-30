//! Crate-wide error and result types.

/// Errors returned by this crate.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The host's state store failed.
    ///
    /// Carries the host's message verbatim: this crate does not know what backs
    /// the store, so it has nothing better to say about why it failed.
    #[error("sync state store failed for {key}: {message}")]
    Store {
        /// Key the operation was for.
        key: String,
        /// Failure as the host reported it.
        message: String,
    },

    /// A Composio action could not be run, or the provider refused it.
    ///
    /// A provider reading a page of a user's mailbox has nothing useful to do
    /// with a half-answer, so a refused action is an error here even though the
    /// module's own execute member reports one as a successful reply.
    #[error("action `{action}` failed: {message}")]
    Action {
        /// Action slug that failed.
        action: String,
        /// What the failure was.
        message: String,
    },

    /// Stored state could not be read back into its type.
    ///
    /// Separate from [`Error::Store`] because the two mean opposite things: a
    /// store failure is usually transient, while this means the persisted shape
    /// and the code have diverged and retrying will fail identically.
    #[error("sync state for {key} did not match its shape: {message}")]
    Decode {
        /// Key whose value failed to decode.
        key: String,
        /// Deserialization failure.
        message: String,
    },
}

/// The crate's standard result type.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod test;
