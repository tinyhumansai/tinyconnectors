//! Reading Slack's payloads, and rendering what they carry.
//!
//! Split from the walk beside it because the two answer different questions.
//! `slack.rs` decides *where to read next*; this decides *what a page said*.
//! Neither needs to know how the other works, and the parsing half is the half
//! worth testing against a payload rather than against a sequence of calls.

use std::collections::BTreeMap;

use serde_json::Value;
use tinyconnectors_bus::records::ConnectorRecord;

use super::slack::Channel;
use super::slack_files::file_records_from;
use crate::Error;
use crate::pipeline::{first_array, pick_str};

/// Longest message body kept, in characters.
///
/// Matches the cap the shared page reader applies. A Slack message is rarely
/// anywhere near it; a pasted stack trace is, and one such record can outweigh
/// a hundred useful ones in both storage and the attention of anything reading
/// them back.
const MAX_BODY_CHARS: usize = 20_000;

/// Slack error codes that mean no later channel will succeed either.
///
/// Everything else — a channel the account is not in, an archived conversation,
/// a rate limit on one busy channel — is per-channel and must not fail the
/// workspace. These are the connection-level ones, where continuing would spend
/// a request per channel to collect the same failure.
const FATAL_AUTH_ERRORS: &[&str] = &[
    "invalid_auth",
    "not_authed",
    "token_revoked",
    "token_expired",
    "account_inactive",
    "missing_scope",
];

/// The messages in a history or thread payload, across Composio's envelopes.
pub(super) fn messages_in(payload: &Value) -> Vec<Value> {
    first_array(
        payload,
        &["/data/messages", "/messages", "/data/data/messages"],
    )
}

/// The timestamp of a message that has replies, if it has any.
pub(super) fn has_replies(message: &Value) -> Option<String> {
    let replies = message
        .get("reply_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    (replies > 0).then(|| pick_str(message, &["ts"]))?
}

/// Whether a failure means the whole connection is unusable.
pub(super) fn is_fatal(error: &Error) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    FATAL_AUTH_ERRORS.iter().any(|code| message.contains(code))
}

/// Slack's next page marker, across the envelopes Composio wraps it in.
///
/// Not [`crate::pipeline::next_page_token`]: that looks for `nextPageToken`,
/// which is Google's spelling. Slack reports `response_metadata.next_cursor`.
pub(super) fn next_cursor(payload: &Value) -> Option<String> {
    [
        "/data/response_metadata/next_cursor",
        "/response_metadata/next_cursor",
        "/data/data/response_metadata/next_cursor",
        "/data/next_cursor",
        "/next_cursor",
    ]
    .iter()
    .find_map(|pointer| payload.pointer(pointer).and_then(Value::as_str))
    .map(str::trim)
    .filter(|cursor| !cursor.is_empty())
    .map(str::to_owned)
}

/// Turn a page of messages into records and the versions they report.
///
/// `skip` names a timestamp to leave out — the parent Slack repeats at the head
/// of its own thread.
pub(super) fn records_from(
    messages: &[Value],
    channel: &Channel,
    users: &BTreeMap<String, String>,
    skip: Option<&str>,
) -> (Vec<ConnectorRecord>, Vec<(String, String)>) {
    let mut records = Vec::with_capacity(messages.len());
    let mut versions = Vec::new();
    for message in messages {
        if let Some(skip) = skip
            && pick_str(message, &["ts"]).as_deref() == Some(skip)
        {
            continue;
        }
        if let Some(record) = record_from(message, channel, users, skip.is_some()) {
            // An edited message re-ingests; an untouched one does not. Slack
            // reports the edit as its own timestamp, which is exactly a version.
            if let Some(edited) = pick_str(message, &["edited.ts"]) {
                versions.push((record.item_id.clone(), edited));
            }
            records.push(record);
        }
        // Files are their own records, and carry no version: `edited.ts`
        // describes the message body, while a file's bytes never change — a
        // re-upload arrives as a new `file_id`, which is a new record anyway.
        records.extend(file_records_from(message, channel, users, skip.is_some()));
    }
    (records, versions)
}

/// Who a message is from, for display.
///
/// An id the directory cannot resolve is still worth keeping: a reader can look
/// `U04AB` up, and collapsing every unresolved author into one word would make
/// messages from different people indistinguishable. This is the same fallback
/// `render` applies to a mention.
pub(super) fn author_of(message: &Value, users: &BTreeMap<String, String>) -> String {
    pick_str(message, &["user"])
        .map(|id| users.get(&id).cloned().unwrap_or(id))
        .or_else(|| pick_str(message, &["username", "bot_id"]))
        .unwrap_or_else(|| "unknown".to_string())
}

/// One message's *text* as a record, or `None` when it carries none.
///
/// A message with no timestamp has no stable id, and one with no text is a join
/// notice, or a file share whose body is the file rather than the message —
/// [`super::slack_files::file_records_from`] is what reads that. Emitting a row
/// here for either would say nothing.
pub(super) fn record_from(
    message: &Value,
    channel: &Channel,
    users: &BTreeMap<String, String>,
    reply: bool,
) -> Option<ConnectorRecord> {
    let ts = pick_str(message, &["ts"])?;
    let text = pick_str(message, &["text"])?;
    let rendered = render(&text, users);
    if rendered.trim().is_empty() {
        return None;
    }

    let author = author_of(message, users);
    let kind = if reply { " (reply)" } else { "" };

    Some(ConnectorRecord {
        // Channel-qualified: a Slack `ts` is unique within a channel, and the
        // run loop dedupes against one flat set for the whole connection.
        item_id: format!("{}:{ts}", channel.id),
        title: format!("#{} — {author}{kind}", channel.name),
        content: truncate(&rendered),
        mime: Some("text/plain".to_string()),
        url: pick_str(message, &["permalink"]).or_else(|| Some(permalink(&channel.id, &ts))),
        updated_at_ms: to_millis(&ts),
        tags: Vec::new(),
    })
}

/// A link back to one message.
///
/// `slack.com` rather than the workspace's own domain: the workspace is not
/// known here, and Slack redirects this form to the right one for whoever
/// opens it.
pub(super) fn permalink(channel: &str, ts: &str) -> String {
    format!(
        "https://slack.com/archives/{channel}/p{}",
        ts.replace('.', "")
    )
}

/// Slack's `seconds.microseconds` stamp in milliseconds.
pub(super) fn to_millis(ts: &str) -> Option<i64> {
    let (seconds, fraction) = ts.split_once('.').unwrap_or((ts, "0"));
    let seconds: i64 = seconds.parse().ok()?;
    // Three digits of the fraction are milliseconds; a shorter one is padded
    // rather than misread as a smaller number.
    let millis: i64 = format!("{fraction:0<3}")
        .chars()
        .take(3)
        .collect::<String>()
        .parse()
        .unwrap_or(0);
    Some(seconds * 1000 + millis)
}

/// Message text with Slack's reference syntax resolved.
///
/// `<@U04AB>` is a user, `<#C07XY|deploys>` a channel, `<https://…|label>` a
/// link. Left raw they are the bulk of what makes ingested Slack unreadable —
/// and an unresolved id is not something a reader can look up later.
pub(super) fn render(raw: &str, users: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;

    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('>') else {
            // An unmatched `<` is literal text, not a broken reference.
            out.push_str(&rest[start..]);
            return out;
        };
        let (token, remainder) = after.split_at(end);
        out.push_str(&resolve(token, users));
        rest = &remainder[1..];
    }
    out.push_str(rest);
    out
}

/// One `<…>` reference, without its angle brackets.
pub(super) fn resolve(token: &str, users: &BTreeMap<String, String>) -> String {
    let (target, label) = token.split_once('|').unwrap_or((token, ""));
    match target.as_bytes().first() {
        // A user: the directory's name, the inline label, or the bare id.
        Some(b'@') => {
            let id = &target[1..];
            let name = users
                .get(id)
                .map(String::as_str)
                .or(Some(label).filter(|label| !label.is_empty()))
                .unwrap_or(id);
            format!("@{name}")
        }
        // A channel: Slack usually inlines the name, so prefer it.
        Some(b'#') => {
            let id = &target[1..];
            let name = if label.is_empty() { id } else { label };
            format!("#{name}")
        }
        // A link: the label if it has one, else the target itself.
        _ if label.is_empty() => target.to_string(),
        _ => label.to_string(),
    }
}

/// Cap a body at [`MAX_BODY_CHARS`], on a character boundary.
pub(super) fn truncate(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= MAX_BODY_CHARS {
        return text.to_string();
    }
    text.chars().take(MAX_BODY_CHARS).collect::<String>()
}

#[cfg(test)]
#[path = "slack_parse_tests.rs"]
mod test;
