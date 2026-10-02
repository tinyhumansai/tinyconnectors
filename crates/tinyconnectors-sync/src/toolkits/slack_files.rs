//! Slack file shares as records.
//!
//! Split from `slack_parse.rs` because a file is not a message. The message
//! half asks what someone *said*; this asks what they *attached*, and the two
//! have different ids, different bodies and different reasons to exist.
//!
//! Metadata only. The record names the file, links to it, and carries whatever
//! text Slack already handed over in `preview` — it does not fetch the bytes.
//! Reading those would need a curated file-read action, which the catalog does
//! not have, plus the `files:read` scope, which every existing connection would
//! have to be re-authorised to gain. The name and the link are what turn "my
//! screenshot vanished" into something a reader can follow.

use std::collections::BTreeMap;

use serde_json::Value;
use tinyconnectors_bus::records::ConnectorRecord;

use super::slack::Channel;
use super::slack_parse::{author_of, to_millis, truncate};
use crate::pipeline::pick_str;

/// One record per file attached to `message`.
///
/// Empty for the overwhelming majority of messages, which carry no files at
/// all, and for a message with no timestamp — without one there is no stable id
/// to hang a file off.
pub(super) fn file_records_from(
    message: &Value,
    channel: &Channel,
    users: &BTreeMap<String, String>,
    reply: bool,
) -> Vec<ConnectorRecord> {
    let Some(ts) = pick_str(message, &["ts"]) else {
        return Vec::new();
    };
    let Some(files) = message.get("files").and_then(Value::as_array) else {
        return Vec::new();
    };

    let shared_by = author_of(message, users);
    let kind = if reply { " (reply)" } else { "" };
    files
        .iter()
        .filter_map(|file| record_for(file, channel, &ts, users, &shared_by, kind))
        .collect()
}

/// Who uploaded a file, when Slack says.
///
/// A file carries its own `user`, and it is not always the one who posted the
/// message: re-sharing someone else's upload, or a bot posting a file a person
/// made, both put two different people on one message. Naming the poster in
/// that case credits the wrong person.
fn uploader_of(file: &Value, users: &BTreeMap<String, String>) -> Option<String> {
    pick_str(file, &["user"]).map(|id| users.get(&id).cloned().unwrap_or(id))
}

/// One file as a record, or `None` when it cannot be identified.
///
/// Two things are required. The id, because it is the dedupe key: a file
/// without one would re-ingest as something new on every single run. And
/// *something to call it* — `name` normally, `title` when Slack sent only that
/// — because a record a reader cannot recognise is barely better than the drop
/// this module exists to undo. A payload with neither is a tombstone for a file
/// that has since been deleted, and skipping it is right.
fn record_for(
    file: &Value,
    channel: &Channel,
    ts: &str,
    users: &BTreeMap<String, String>,
    shared_by: &str,
    kind: &str,
) -> Option<ConnectorRecord> {
    let id = pick_str(file, &["id"])?;
    let name = pick_str(file, &["name", "title"])?;
    let author = uploader_of(file, users).unwrap_or_else(|| shared_by.to_string());

    Some(ConnectorRecord {
        // The message timestamp alone is not unique across channels, and one
        // message can carry several files, so the id needs all three parts.
        item_id: format!("{}:{ts}:{id}", channel.id),
        title: format!("#{} — {author} shared {name}{kind}", channel.name),
        content: truncate(&describe(file, &name)),
        // The mime of `content`, which is the text below — not the mime of the
        // file, whose bytes this record does not carry. The file's own type is
        // part of the body instead.
        mime: Some("text/plain".to_string()),
        url: pick_str(file, &["permalink"]),
        // A file uploaded long after the message it hangs off — an edit, a
        // later addition to a thread — is better placed by its own clock. The
        // message timestamp is the fallback, not the first choice.
        updated_at_ms: file_millis(file).or_else(|| to_millis(ts)),
        tags: Vec::new(),
    })
}

/// What the record says about a file.
///
/// The name always, the human title when it differs from the filename, the
/// type, and Slack's own `preview` when there is one — snippets and posts carry
/// their opening lines there, which is real content for free.
fn describe(file: &Value, name: &str) -> String {
    let mut parts = vec![name.to_string()];
    if let Some(title) = pick_str(file, &["title"]).filter(|title| title != name) {
        parts.push(title);
    }
    if let Some(mimetype) = pick_str(file, &["mimetype"]) {
        parts.push(mimetype);
    }
    if let Some(preview) = pick_str(file, &["preview"]) {
        parts.push(preview);
    }
    parts.join("\n")
}

/// When a file last changed, in milliseconds.
///
/// `updated` before `created`: `ConnectorRecord::updated_at_ms` is the upstream
/// *last-modified* time, and a Slack Post edited after it was uploaded reports
/// both. Whole seconds either way.
///
/// This does not make an edited file re-ingest — file records emit no versions,
/// and the walk's high-water mark reads the message `ts`, not this. It is the
/// field meaning what it says.
fn file_millis(file: &Value) -> Option<i64> {
    to_millis(&pick_str(file, &["updated", "created"])?)
}

#[cfg(test)]
#[path = "slack_files_tests.rs"]
mod test;
