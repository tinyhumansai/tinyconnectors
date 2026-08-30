//! GitHub repository payloads.

use serde::{Deserialize, Serialize};

/// One repository visible to a connected GitHub account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposioGithubRepo {
    /// Owning user or organization login.
    pub owner: String,
    /// Repository name, without the owner.
    pub repo: String,
    /// `owner/repo`, as GitHub renders it.
    #[serde(rename = "fullName")]
    pub full_name: String,
    /// Whether the repository is private, when the listing says.
    #[serde(default)]
    pub private: Option<bool>,
    /// Default branch name, when the listing says.
    #[serde(rename = "defaultBranch", default)]
    pub default_branch: Option<String>,
    /// Browser URL for the repository, when the listing says.
    #[serde(rename = "htmlUrl", default)]
    pub html_url: Option<String>,
}

/// Response body of `GET /agent-integrations/composio/github/repos`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposioGithubReposResponse {
    /// Connection the repositories were listed through.
    #[serde(rename = "connectionId")]
    pub connection_id: String,
    /// Repositories that connection can see.
    #[serde(default)]
    pub repositories: Vec<ComposioGithubRepo>,
}
