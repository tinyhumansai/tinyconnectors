//! Cutting quoted chains and boilerplate out of message bodies.

/// Text that marks the start of footer territory when it appears on a line.
///
/// Deliberately short and unambiguous: each entry is something that could not
/// reasonably appear inside a message someone wrote. A looser list would cut
/// into real prose, which is the failure that loses content silently.
const FOOTER_TRIGGERS: &[&str] = &[
    "unsubscribe",
    "view in browser",
    "view this email in your browser",
    "view it in your browser",
    "update your email settings",
    "manage your subscription",
    "manage preferences",
    "email preferences",
    "you are receiving this email because",
    "you received this email because",
    "you're receiving this email because",
    "to stop receiving",
    "all rights reserved",
    "© 20",
    "(c) 20",
    "copyright 20",
    "powered by mailchimp",
    "sent via sendgrid",
    "this email and any files",
    "confidentiality notice",
    "if you are not the intended recipient",
    "this communication may contain",
];

/// How many consecutive quoted lines mean a reply chain rather than a citation.
///
/// Three. One or two `>` lines is someone quoting a sentence to answer it;
/// three in a row is a client that pasted the whole parent message.
const QUOTED_RUN_THRESHOLD: u32 = 3;

/// Clean a message body for ingestion.
///
/// Drops the quoted reply chain, then the footer, then collapses the blank
/// runs the first two passes leave behind.
///
/// The order matters: a quoted-chain preamble sitting *below* a "view in
/// browser" line still gets cut on its own merits, because the reply pass runs
/// first and does not depend on the footer pass having found anything.
#[must_use]
pub fn clean_body(raw: &str) -> String {
    let without_replies = drop_reply_chain(raw);
    let without_footer = drop_footer_noise(&without_replies);
    collapse_blank_runs(without_footer.trim())
}

/// Cut everything from the start of a quoted reply chain.
///
/// The parent message is already in memory in its own right — it arrived as its
/// own record. Keeping the quoted copy ingests it a second time, attached to
/// the wrong sender and date.
#[must_use]
pub fn drop_reply_chain(text: &str) -> String {
    let mut offset = 0usize;
    let mut quoted_run_start: Option<usize> = None;
    let mut quoted_run_length = 0u32;

    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();

        if is_reply_preamble(&lower) {
            return cut_at(text, offset);
        }

        if trimmed.starts_with('>') {
            if quoted_run_start.is_none() {
                quoted_run_start = Some(offset);
                quoted_run_length = 1;
            } else {
                quoted_run_length += 1;
            }
            if quoted_run_length >= QUOTED_RUN_THRESHOLD {
                return cut_at(text, quoted_run_start.unwrap_or(offset));
            }
        } else if !trimmed.is_empty() {
            // A non-empty unquoted line ends the run. Blank lines do not, since
            // senders routinely interleave them inside a quoted block.
            quoted_run_start = None;
            quoted_run_length = 0;
        }

        offset += line.len();
    }
    text.to_string()
}

/// Whether a line explicitly introduces a quoted or forwarded message.
fn is_reply_preamble(lower: &str) -> bool {
    (lower.starts_with("on ") && lower.contains(" wrote:"))
        || lower.contains("---------- forwarded message")
        || lower.contains("----- original message")
        || lower.contains("--------- original message")
        || lower.contains("--- forwarded by")
}

/// Cut everything from the first line that carries a footer trigger.
#[must_use]
pub fn drop_footer_noise(text: &str) -> String {
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let lower = line.to_ascii_lowercase();
        if FOOTER_TRIGGERS.iter().any(|trigger| lower.contains(trigger)) {
            return cut_at(text, offset);
        }
        offset += line.len();
    }
    text.to_string()
}

/// Truncate `text` at a byte offset without splitting a character.
///
/// Slicing directly would panic mid-character, and a message body is exactly
/// where a multi-byte character turns up — an em dash, a name, an emoji.
fn cut_at(text: &str, offset: usize) -> String {
    let mut cut = offset.min(text.len());
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    text[..cut].trim_end().to_string()
}

/// Collapse runs of blank lines into one, and trim the ends.
#[must_use]
pub fn collapse_blank_runs(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank_run = 0u32;

    for line in text.lines() {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim().to_string()
}

/// Cap a body at `max_chars` characters, marking where it was cut.
///
/// Counted in characters, not bytes, so the cap means the same thing for a
/// message in any language — and so the slice can never split a character.
/// The marker matters: a body that just stops reads to a model as the message
/// having ended there, and it will answer as if the rest did not exist.
#[must_use]
pub fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let kept: String = text.chars().take(max_chars).collect();
    format!("{}\n\n[truncated]", kept.trim_end())
}
