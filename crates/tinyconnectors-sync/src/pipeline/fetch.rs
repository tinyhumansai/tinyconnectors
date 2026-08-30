//! Reading one page from a provider, declaratively.
//!
//! Every toolkit's page read is the same five decisions — which action, where
//! the items are, which field is the id, the title, the body — differing only
//! in the strings. A [`PageSpec`] states those strings; this module does the
//! reading. Writing each one out by hand instead produced five copies of the
//! same envelope-unwrapping, which is exactly where they drifted.

use serde_json::{Value, json};
use tinyconnectors_bus::ConnectorRecord;

use super::json::{first_array, next_page_token, pick_str};
use super::run::ProviderPage;
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
    /// The argument naming how many items to return.
    pub page_size_arg: &'static str,
    /// The argument naming where to resume.
    pub cursor_arg: &'static str,
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
    // Never ask for more than the run can use: a page of a hundred when the
    // limit leaves room for three is three ingested and ninety-seven paid for.
    let page_size = context.limits.max_items.clamp(1, 100);
    let mut arguments = json!({ spec.page_size_arg: page_size });
    if let Some(cursor) = cursor {
        arguments[spec.cursor_arg] = Value::String(cursor.to_string());
    }

    let payload = context.run(spec.action, arguments).await?;
    Ok(page_from(&payload, spec))
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
            content: pick_str(item, spec.content_paths).unwrap_or_else(|| item.to_string()),
            mime: Some("text/plain".to_string()),
            url: pick_str(item, spec.url_paths),
            updated_at_ms: None,
            tags: Vec::new(),
        });
    }

    ProviderPage {
        records,
        versions,
        next_cursor: next_page_token(payload),
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
#[path = "fetch_test.rs"]
mod test;
