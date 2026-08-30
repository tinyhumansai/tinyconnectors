//! The repository listing GitHub-scoped triggers bind to.
//!
//! GitHub is the one toolkit whose triggers are not global: a subscription is
//! per repository, so enabling one needs the list of repositories the
//! connection can see. That listing is Composio's shape, not GitHub's, and it
//! is small enough that a dedicated family is clearer than folding it into
//! [`super::triggers`].

mod types;

pub use types::{ComposioGithubRepo, ComposioGithubReposResponse, ComposioListGithubReposRequest};

#[cfg(test)]
mod test;
