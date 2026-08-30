//! Scope classification and the curated catalog type.

use serde::{Deserialize, Serialize};

/// How invasive an action is.
///
/// Ordered by consequence, which is what makes a user preference of "read only"
/// expressible as a threshold rather than a set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolScope {
    /// Pure reads — get, fetch, list, search.
    Read,
    /// Creates or changes the user's data — send, create, update, reply.
    Write,
    /// Destructive or permission-changing — delete, trash, share, revoke.
    Admin,
}

impl ToolScope {
    /// The stable wire name used in preferences and catalogs.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Admin => "admin",
        }
    }

    /// Whether this scope is permitted when the user has allowed up to `limit`.
    ///
    /// The ordering is the point: allowing write allows read, because a user
    /// who consented to changes has necessarily consented to looking.
    #[must_use]
    pub fn is_allowed_by(self, limit: Self) -> bool {
        self <= limit
    }
}

/// One entry in a provider's curated catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CuratedTool {
    /// Composio action slug, e.g. `"GMAIL_SEND_EMAIL"`.
    pub slug: &'static str,
    /// How invasive the action is.
    pub scope: ToolScope,
}

/// Classify an action that no curated catalog covers.
///
/// Prefer a curated classification whenever one exists — this is the fallback
/// for toolkits nobody has hand-tuned yet.
///
/// Destructive verbs are tested first on purpose: `GMAIL_MODIFY_LABELS`
/// contains no write verb but does contain `MODIFY_LABELS`, and checking the
/// write list first would classify several destructive actions as merely
/// mutating. The default is [`ToolScope::Read`] because an action naming no
/// recognized verb is far more often a query than a deletion.
#[must_use]
pub fn classify_unknown(slug: &str) -> ToolScope {
    const ADMIN: &[&str] = &[
        "DELETE",
        "TRASH",
        "REMOVE",
        "MODIFY_LABELS",
        "SHARE",
        "REVOKE",
        "DESTROY",
    ];
    const WRITE: &[&str] = &[
        "SEND", "CREATE", "UPDATE", "REPLY", "APPEND", "INSERT", "ADD", "POST", "PATCH", "WRITE",
        "DRAFT",
    ];

    let upper = slug.to_ascii_uppercase();
    if ADMIN.iter().any(|verb| upper.contains(verb)) {
        ToolScope::Admin
    } else if WRITE.iter().any(|verb| upper.contains(verb)) {
        ToolScope::Write
    } else {
        ToolScope::Read
    }
}

/// Find `slug` in a curated catalog, case-insensitively.
#[must_use]
pub fn find_curated<'a>(catalog: &'a [CuratedTool], slug: &str) -> Option<&'a CuratedTool> {
    catalog
        .iter()
        .find(|tool| tool.slug.eq_ignore_ascii_case(slug))
}

/// The toolkit an action slug belongs to.
///
/// Most slugs are `<TOOLKIT>_<VERB>_…`, so the first segment is the toolkit.
/// A few toolkit names contain underscores themselves, and splitting those on
/// the first underscore yields a toolkit nothing matches — which silently drops
/// every one of that toolkit's actions from a connected-toolkit check. Those
/// are listed explicitly.
#[must_use]
pub fn toolkit_from_slug(slug: &str) -> Option<String> {
    const MULTI_SEGMENT_TOOLKITS: &[(&str, &str)] = &[
        ("MICROSOFT_TEAMS_", "microsoft_teams"),
        ("ONE_DRIVE_", "one_drive"),
        ("ZOHO_MAIL_", "zoho_mail"),
    ];

    let trimmed = slug.trim();
    if trimmed.is_empty() {
        return None;
    }

    let upper = trimmed.to_ascii_uppercase();
    for (prefix, toolkit) in MULTI_SEGMENT_TOOLKITS {
        if upper.starts_with(prefix) {
            return Some((*toolkit).to_string());
        }
    }

    let prefix = trimmed.split('_').next()?;
    (!prefix.is_empty()).then(|| prefix.to_ascii_lowercase())
}
