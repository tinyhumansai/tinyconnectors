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
/// # The order of the checks is the whole design
///
/// 1. **Any destructive word, anywhere, wins.** `GMAIL_MODIFY_LABELS` contains
///    no write verb but does contain `MODIFY_LABELS`, and `ADD_AND_REMOVE`
///    contains both — a scan that stopped at the first write verb would offer
///    either to a user who allowed only changes, not deletions.
/// 2. **Then the leading verb, if it reads.** `GMAIL_LIST_DRAFTS` is a read
///    that a plain substring scan calls a write, because the *noun* is
///    `DRAFTS`. Verbs describe the action; nouns describe what it acts on, and
///    only the verb says how invasive it is.
/// 3. **Then any write word.**
/// 4. **Otherwise read**, because an action naming no recognized verb is far
///    more often a query than a deletion.
#[must_use]
pub fn classify_unknown(slug: &str) -> ToolScope {
    /// Words that make an action destructive wherever they appear.
    const ADMIN: &[&str] = &[
        "DELETE",
        "TRASH",
        "REMOVE",
        "MODIFY_LABELS",
        "SHARE",
        "REVOKE",
        "DESTROY",
    ];
    /// Verbs that only ever read, when they lead the action.
    const READ_VERBS: &[&str] = &[
        "GET", "LIST", "FETCH", "SEARCH", "READ", "FIND", "QUERY", "RETRIEVE", "DOWNLOAD",
        "EXPORT", "COUNT", "CHECK",
    ];
    /// Words that make an action mutating.
    const WRITE: &[&str] = &[
        "SEND", "CREATE", "UPDATE", "REPLY", "APPEND", "INSERT", "ADD", "POST", "PATCH", "WRITE",
        "DRAFT",
    ];

    let upper = slug.trim().to_ascii_uppercase();
    if ADMIN.iter().any(|word| upper.contains(word)) {
        return ToolScope::Admin;
    }
    if leading_verb(&upper).is_some_and(|verb| READ_VERBS.contains(&verb)) {
        return ToolScope::Read;
    }
    if WRITE.iter().any(|word| upper.contains(word)) {
        return ToolScope::Write;
    }
    ToolScope::Read
}

/// The verb of an upper-cased action slug.
///
/// Slugs are `<TOOLKIT>_<VERB>_<NOUN…>`, so the verb is the second segment —
/// and the segment after that for the few toolkits whose own name contains an
/// underscore, which is why the toolkit is stripped rather than assumed to be
/// one segment.
fn leading_verb(upper: &str) -> Option<&str> {
    let toolkit = toolkit_from_slug(upper)?;
    let remainder = upper.get(toolkit.len()..)?.trim_start_matches('_');
    remainder.split('_').next().filter(|verb| !verb.is_empty())
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
