//! Unit tests for body cleaning.
//!
//! The asymmetry these guard: leaving noise in costs tokens and some search
//! quality, while cutting into a message loses something the user wrote and
//! will never know is missing. Where a case is ambiguous, the test asserts that
//! the content survives.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{clean_body, collapse_blank_runs, drop_footer_noise, drop_reply_chain, truncate};

#[test]
fn cuts_a_quoted_reply_preamble() {
    let body = "Sure, that works.\n\nOn Tue, 3 Mar 2026, Ada wrote:\n> the original question\n";
    assert_eq!(drop_reply_chain(body), "Sure, that works.");
}

#[test]
fn cuts_a_forwarded_message_separator() {
    for separator in [
        "---------- Forwarded message ----------",
        "----- Original Message -----",
        "--- Forwarded by Ada on 2026 ---",
    ] {
        let body = format!("My note.\n\n{separator}\nold content\n");
        assert_eq!(drop_reply_chain(&body), "My note.", "{separator}");
    }
}

#[test]
fn cuts_a_run_of_quoted_lines_even_without_a_preamble() {
    // Some clients de-quote on send, leaving the chain with no introduction.
    let body = "Agreed.\n\n> one\n> two\n> three\n> four\n";
    assert_eq!(drop_reply_chain(body), "Agreed.");
}

#[test]
fn keeps_a_short_quotation_someone_is_answering() {
    // One or two quoted lines is a person quoting a sentence to reply to it —
    // which is content, not a chain.
    let body = "> should we ship on Friday?\n\nYes, let's.";
    assert_eq!(drop_reply_chain(body), body);
}

#[test]
fn a_blank_line_does_not_break_a_quoted_run() {
    // Senders routinely interleave blank lines inside a quoted block.
    let body = "Noted.\n\n> one\n\n> two\n\n> three\n";
    assert_eq!(drop_reply_chain(body), "Noted.");
}

#[test]
fn an_unquoted_line_resets_the_run() {
    let body = "> a\n> b\nstill writing\n> c\n> d\nend";
    assert_eq!(
        drop_reply_chain(body),
        body,
        "two runs of two are not a chain"
    );
}

#[test]
fn cuts_at_the_first_footer_trigger() {
    let body = "The actual message.\n\nUnsubscribe | View in browser\n123 Street";
    assert_eq!(drop_footer_noise(body), "The actual message.");
}

#[test]
fn recognizes_the_common_footer_shapes() {
    for footer in [
        "Unsubscribe",
        "You are receiving this email because you signed up",
        "© 2026 Example Inc.",
        "All rights reserved",
        "CONFIDENTIALITY NOTICE",
        "If you are not the intended recipient, delete this",
    ] {
        let body = format!("Real content.\n\n{footer}\n");
        assert_eq!(drop_footer_noise(&body), "Real content.", "{footer}");
    }
}

#[test]
fn leaves_prose_that_merely_resembles_a_footer() {
    // The triggers must not fire on ordinary writing.
    let body = "Can you copyright a recipe? I don't think all rights are reserved by default.";
    assert!(
        drop_footer_noise(body).len() > 40,
        "ordinary prose must survive: {}",
        drop_footer_noise(body)
    );
}

#[test]
fn collapses_runs_of_blank_lines() {
    assert_eq!(collapse_blank_runs("a\n\n\n\nb"), "a\n\nb");
    assert_eq!(collapse_blank_runs("  \n\na\n\n  \n\nb\n\n\n"), "a\n\nb");
}

#[test]
fn cleaning_applies_every_pass_in_order() {
    let body = "\
Here is the real message.


On Tue, Ada wrote:
> old thread

Unsubscribe
";
    assert_eq!(clean_body(body), "Here is the real message.");
}

#[test]
fn a_reply_preamble_below_a_footer_line_is_still_cut() {
    // The reply pass runs first and does not depend on the footer pass having
    // found anything.
    let body = "Message.\n\nOn Tue, Ada wrote:\n> quoted\n\nUnsubscribe\n";
    assert_eq!(clean_body(body), "Message.");
}

#[test]
fn cleaning_an_ordinary_message_changes_nothing_but_whitespace() {
    let body = "Hello,\n\nCan we move the meeting to Thursday?\n\nThanks,\nAda";
    assert_eq!(clean_body(body), body);
}

#[test]
fn cleaning_something_that_is_entirely_noise_yields_nothing() {
    assert_eq!(clean_body("Unsubscribe | View in browser"), "");
    assert_eq!(clean_body(""), "");
    assert_eq!(clean_body("   \n\n  "), "");
}

#[test]
fn a_cut_never_splits_a_character() {
    // A message body is exactly where a multi-byte character turns up. Slicing
    // on a byte offset mid-character panics.
    let body = "Réponse — c'est prêt ✅\n> quoted\n> quoted\n> quoted\n";
    let cleaned = drop_reply_chain(body);
    assert_eq!(cleaned, "Réponse — c'est prêt ✅");
}

#[test]
fn truncation_counts_characters_not_bytes() {
    // So the cap means the same thing in any language, and the slice cannot
    // split a character.
    let body = "ééééééééééé";
    let truncated = truncate(body, 5);
    assert!(truncated.starts_with("ééééé"));
    assert!(truncated.contains("[truncated]"));
}

#[test]
fn truncation_marks_where_it_cut() {
    // A body that just stops reads to a model as the message having ended
    // there, and it answers as though the rest did not exist.
    let long = "word ".repeat(100);
    let truncated = truncate(&long, 50);
    assert!(truncated.ends_with("[truncated]"));
}

#[test]
fn a_short_body_is_returned_unchanged() {
    assert_eq!(truncate("short", 50), "short");
    assert_eq!(truncate("", 50), "");
}
