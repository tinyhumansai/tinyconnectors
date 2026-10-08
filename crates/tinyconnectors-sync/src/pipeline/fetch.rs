//! Reading one page from a provider, declaratively.
//!
//! Every toolkit's page read is the same five decisions — which action, where
//! the items are, which field is the id, the title, the body — differing only
//! in the strings. A [`PageSpec`] states those strings; this module does the
//! reading. Writing each one out by hand instead produced five copies of the
//! same envelope-unwrapping, which is exactly where they drifted.

use chrono::{TimeDelta, Utc};
use serde_json::{Value, json};
use tinyconnectors_bus::ConnectorRecord;

use super::json::{end_cursor, first_array, flag_at, pick_str, token_at};
use super::run::ProviderPage;
use super::sender::sender_of;
use crate::Result;
use crate::clean::{clean_body, truncate};
use crate::provider::ProviderContext;

/// How one toolkit's page read is shaped.
#[derive(Debug, Clone, Copy)]
pub struct PageSpec {
    /// The action that reads a page.
    pub action: &'static str,
    /// JSON Pointers to try for the item array, in order.
    pub item_pointers: &'static [&'static str],
    /// Dotted paths to try for an item's stable id.
    pub id_paths: &'static [&'static str],
    /// Dotted paths to try for an item's title.
    pub title_paths: &'static [&'static str],
    /// Dotted paths to try for an item's body text.
    pub content_paths: &'static [&'static str],
    /// Dotted paths to try for a canonical link back to the item.
    pub url_paths: &'static [&'static str],
    /// Dotted paths to try for an item's version, when the source reports one.
    pub version_paths: &'static [&'static str],
    /// Dotted paths to try for who wrote the item (`Name <address>` or a bare
    /// address), falling back to a `From` header under `payload.headers`.
    /// Empty for a toolkit whose items have no sender.
    pub sender_paths: &'static [&'static str],
    /// Arguments every page read sends unchanged, as `(name, value)` pairs.
    ///
    /// For an action that reads nothing without them: GitHub's issue search
    /// rejects a request that carries no query.
    pub fixed_arguments: &'static [(&'static str, &'static str)],
    /// The argument naming how many items to return.
    pub page_size_arg: &'static str,
    /// The argument naming where to resume.
    pub cursor_arg: &'static str,
    /// How the page after this one is named.
    pub paging: Paging,
    /// How the toolkit expresses "no older than N days" on a page read, when
    /// it can. `None` reads without a lower bound whatever the limits say.
    ///
    /// A window the *provider* applies, never one applied here after the
    /// fact: reading everything and dropping the old would spend exactly the
    /// requests the bound exists to save.
    pub depth_window: Option<DepthWindow>,
    /// Whether to strip quoted chains and boilerplate from the body.
    ///
    /// True for message-shaped toolkits, where a body carries the thread it
    /// replied to and a footer repeated across every message the user has ever
    /// received. False for issue and task toolkits, whose descriptions are
    /// written once and quote nothing — running the pass there would only risk
    /// cutting a line that happens to look like a footer.
    pub clean_bodies: bool,
}

/// Longest body kept, in characters.
///
/// A cap rather than no limit: a single thread can run to hundreds of
/// kilobytes, and one such record can outweigh a hundred useful ones in both
/// storage and the attention of anything reading them back.
const MAX_BODY_CHARS: usize = 20_000;

/// How a toolkit names the page after the one just read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paging {
    /// The payload names the next page with a token, sent back verbatim as the
    /// cursor: Gmail's `nextPageToken`, Notion's `next_cursor`.
    Token {
        /// JSON Pointers to try for the next page's token, in order.
        next: &'static [&'static str],
    },
    /// The payload is a GraphQL connection. The next page starts after its
    /// `pageInfo.endCursor`, while `pageInfo.hasNextPage` is true.
    PageInfo {
        /// JSON Pointers to try for the connection's `pageInfo`, in order.
        page_info: &'static [&'static str],
    },
    /// Pages are numbered, and the payload may not say whether another
    /// follows.
    ///
    /// A `last_page` flag is believed when the payload carries one; otherwise
    /// another page follows while pages come back full. A page with nothing on
    /// it is the last either way.
    ///
    /// GitHub's search counts from one and names no next page. It answers a
    /// page past the results it serves with an error rather than an empty
    /// page, so a walk has to stop before asking for one. `ClickUp` counts from
    /// zero and flags its last page.
    Numbered {
        /// The number of the first page.
        first: u32,
        /// Results the provider pages through at all, when it stops serving.
        reachable: Option<u32>,
        /// JSON Pointers to try for the flag the payload sets on its last page.
        last_page: &'static [&'static str],
    },
}

/// How a toolkit's page read is told to stop at an age.
///
/// One variant per provider syntax. Gmail and GitHub take search strings, so
/// the bound is a term in the query; a provider whose API has no such argument
/// has no variant and reads unbounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepthWindow {
    /// Gmail search syntax: `query: "after:YYYY/MM/DD"`.
    GmailQueryAfter,
    /// GitHub search syntax: `updated:>=YYYY-MM-DD`, added to the `q` query.
    GithubUpdatedSince,
}

impl DepthWindow {
    /// Add the bound for `days` back from now to `arguments`.
    fn apply(self, arguments: &mut Value, days: u32) {
        match self {
            Self::GmailQueryAfter => {
                arguments["query"] = Value::String(gmail_after_query(days));
            }
            Self::GithubUpdatedSince => {
                // Narrows the query the spec sends, and never stands in for
                // one: an `updated:` term on its own would search every
                // repository on GitHub.
                if let Some(query) = arguments.get("q").and_then(Value::as_str) {
                    let bounded = format!("{query} {}", github_updated_since(days));
                    arguments["q"] = Value::String(bounded);
                }
            }
        }
    }
}

/// The Gmail search term for "newer than `days` days".
///
/// A calendar date in UTC rather than an epoch: Gmail reads `after:` dates in
/// the mailbox's own zone, so the window can land a few hours wide of the mark
/// at either end. That is the right side to err on for a bound whose job is to
/// stop a years-deep backfill, not to cut a day in half.
pub(crate) fn gmail_after_query(days: u32) -> String {
    let since = Utc::now() - TimeDelta::days(i64::from(days));
    format!("after:{}", since.format("%Y/%m/%d"))
}

/// The GitHub search term for "updated in the last `days` days".
///
/// A calendar date, as for Gmail: a bound whose job is to stop a walk through
/// years of issues does not need the hour.
pub(crate) fn github_updated_since(days: u32) -> String {
    let since = Utc::now() - TimeDelta::days(i64::from(days));
    format!("updated:>={}", since.format("%Y-%m-%d"))
}

/// Read one page of `spec` from the connection in `context`.
///
/// # Errors
///
/// Returns [`crate::Error::Action`] when the action fails.
pub async fn fetch_page(
    context: &ProviderContext,
    cursor: Option<&str>,
    spec: &PageSpec,
) -> Result<ProviderPage> {
    fetch_page_with(context, cursor, spec, &[]).await
}

/// [`fetch_page`], plus arguments known only when the page is read, such as the
/// workspace a `ClickUp` walk has reached.
///
/// # Errors
///
/// Returns [`crate::Error::Action`] when the action fails.
pub(crate) async fn fetch_page_with(
    context: &ProviderContext,
    cursor: Option<&str>,
    spec: &PageSpec,
    this_read: &[(&str, Value)],
) -> Result<ProviderPage> {
    // Never ask for more than the run can use: a page of a hundred when the
    // limit leaves room for three is three ingested and ninety-seven paid for.
    let page_size = context.limits.max_items.clamp(1, 100);
    let mut arguments = json!({ spec.page_size_arg: page_size });
    for &(name, value) in spec.fixed_arguments {
        arguments[name] = Value::String(value.to_string());
    }
    for (name, value) in this_read {
        arguments[*name] = value.clone();
    }
    let numbered = match spec.paging {
        Paging::Token { .. } | Paging::PageInfo { .. } => {
            if let Some(cursor) = cursor {
                arguments[spec.cursor_arg] = Value::String(cursor.to_string());
            }
            None
        }
        Paging::Numbered { first, .. } => {
            let number = page_number(cursor, first);
            arguments[spec.cursor_arg] = Value::from(number);
            Some(number)
        }
    };
    if let (Some(days), Some(window)) = (context.limits.depth_days, spec.depth_window) {
        window.apply(&mut arguments, days);
    }

    let payload = context.run(spec.action, arguments).await?;
    let mut page = page_from(&payload, spec);
    if let (
        Some(number),
        Paging::Numbered {
            first,
            reachable,
            last_page,
        },
    ) = (numbered, spec.paging)
    {
        page.next_cursor = next_page_number(
            number,
            first,
            reachable,
            item_count(&payload, spec),
            page_size,
            flag_at(&payload, last_page),
        )
        .map(|next| next.to_string());
    }
    Ok(page)
}

/// The page a numbered cursor names.
///
/// No cursor is the first page, and so is a cursor that is not a page number
/// at or past `first`. Starting that walk over costs one re-read, which the
/// seen-set turns into skips; refusing to read would stall the connection for
/// good.
fn page_number(cursor: Option<&str>, first: u32) -> u32 {
    cursor
        .and_then(|cursor| cursor.trim().parse::<u32>().ok())
        .filter(|number| *number >= first)
        .unwrap_or(first)
}

/// The page after `page`, or `None` when `page` is the last worth asking for.
///
/// A page with nothing on it is the last. A payload's `last` flag is believed
/// when there is one; without it, a short page is the last. So is a page that
/// reaches `reachable`, since the provider answers the one after with an
/// error. A full page that happens to end the results costs one empty read to
/// discover when the payload does not say.
fn next_page_number(
    page: u32,
    first: u32,
    reachable: Option<u32>,
    items: usize,
    page_size: usize,
    last: Option<bool>,
) -> Option<u32> {
    if items == 0 || last == Some(true) || (last.is_none() && items < page_size) {
        return None;
    }
    if let Some(reachable) = reachable {
        let pages_read = u64::from(page.saturating_sub(first)) + 1;
        let read = pages_read.saturating_mul(u64::try_from(page_size).unwrap_or(u64::MAX));
        if read >= u64::from(reachable) {
            return None;
        }
    }
    page.checked_add(1)
}

/// How many items `payload` holds under the spec's pointers.
///
/// Counted before any item is dropped for want of an id: whether a page came
/// back full is a question about the provider's page, not the records kept.
fn item_count(payload: &Value, spec: &PageSpec) -> usize {
    spec.item_pointers
        .iter()
        .find_map(|pointer| payload.pointer(pointer).and_then(Value::as_array))
        .map_or(0, Vec::len)
}

/// Turn a provider payload into a page.
///
/// An item with no derivable id is dropped: the id is the dedupe key, and a
/// record without one re-ingests as new on every run, filling the user's memory
/// with copies of the same thing.
fn page_from(payload: &Value, spec: &PageSpec) -> ProviderPage {
    let items = first_array(payload, spec.item_pointers);
    let mut records = Vec::with_capacity(items.len());
    let mut versions = Vec::new();

    for item in &items {
        let Some(item_id) = pick_str(item, spec.id_paths) else {
            continue;
        };
        if let Some(version) = pick_str(item, spec.version_paths) {
            versions.push((item_id.clone(), version));
        }
        records.push(ConnectorRecord {
            item_id,
            title: pick_str(item, spec.title_paths).unwrap_or_default(),
            // Falls back to the whole item: a record with no recognizable body
            // field still carries something an agent can read, which beats
            // ingesting an empty one.
            content: body(
                &pick_str(item, spec.content_paths).unwrap_or_else(|| item.to_string()),
                spec.clean_bodies,
            ),
            mime: Some("text/plain".to_string()),
            url: pick_str(item, spec.url_paths),
            updated_at_ms: None,
            tags: Vec::new(),
            sender: sender_of(item, spec.sender_paths),
        });
    }

    ProviderPage {
        records,
        versions,
        next_cursor: match spec.paging {
            Paging::Token { next } => token_at(payload, next),
            Paging::PageInfo { page_info } => end_cursor(payload, page_info),
            // Decided by the read, which knows the page it asked for.
            Paging::Numbered { .. } => None,
        },
        requests_used: 1,
    }
}

/// Prepare one item's text for ingestion.
fn body(raw: &str, clean: bool) -> String {
    let text = if clean {
        clean_body(raw)
    } else {
        raw.trim().to_string()
    };
    truncate(&text, MAX_BODY_CHARS)
}

#[cfg(test)]
#[path = "fetch_tests.rs"]
mod test;
