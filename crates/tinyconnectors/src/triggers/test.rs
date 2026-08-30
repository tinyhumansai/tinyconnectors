//! Unit tests for the trigger archive.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;

use serde_json::json;

use super::{DEFAULT_HISTORY_LIMIT, TriggerArchive};

/// A scratch directory that removes itself.
struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "tinyconnectors-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("scratch directory");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn archive(name: &str) -> (TempDir, TriggerArchive) {
    let dir = TempDir::new(name);
    let archive = TriggerArchive::open(&dir.0).expect("opens");
    (dir, archive)
}

#[test]
fn opening_creates_the_archive_directory() {
    let (dir, archive) = archive("open");
    assert!(archive.archive_dir().is_dir());
    assert!(archive.archive_dir().starts_with(&dir.0));
}

#[test]
fn opening_twice_is_fine() {
    let (dir, _archive) = archive("reopen");
    assert!(TriggerArchive::open(&dir.0).is_ok());
}

#[test]
fn records_a_delivery_and_reads_it_back() {
    let (_dir, archive) = archive("record");
    let entry = archive
        .record(
            "gmail",
            "GMAIL_NEW_GMAIL_MESSAGE",
            "evt-1",
            "uuid-1",
            &json!({ "subject": "hi" }),
        )
        .expect("records");

    assert_eq!(entry.toolkit, "gmail");
    assert!(entry.received_at_ms > 0);

    let history = archive.list_recent(None).expect("reads");
    assert_eq!(history.entries.len(), 1);
    assert_eq!(history.entries[0].metadata_id, "evt-1");
    assert_eq!(history.entries[0].payload["subject"], "hi");
    assert_eq!(
        history.current_day_file,
        archive.current_day_file().display().to_string()
    );
}

#[test]
fn returns_the_newest_deliveries_first() {
    let (_dir, archive) = archive("order");
    for index in 0..5 {
        archive
            .record("gmail", "T", &format!("evt-{index}"), "u", &json!({}))
            .expect("records");
    }

    let history = archive.list_recent(None).expect("reads");
    let ids: Vec<_> = history
        .entries
        .iter()
        .map(|entry| entry.metadata_id.as_str())
        .collect();
    assert_eq!(ids, ["evt-4", "evt-3", "evt-2", "evt-1", "evt-0"]);
}

#[test]
fn honours_the_requested_limit() {
    let (_dir, archive) = archive("limit");
    for index in 0..10 {
        archive
            .record("gmail", "T", &format!("evt-{index}"), "u", &json!({}))
            .expect("records");
    }

    let history = archive.list_recent(Some(3)).expect("reads");
    assert_eq!(history.entries.len(), 3);
    assert_eq!(history.entries[0].metadata_id, "evt-9");
}

#[test]
fn a_zero_limit_still_returns_something() {
    // Asking for nothing is a caller mistake, not a request to be answered
    // literally with an empty list that looks like "no triggers ever fired".
    let (_dir, archive) = archive("zero");
    archive
        .record("gmail", "T", "evt-0", "u", &json!({}))
        .expect("records");

    let history = archive.list_recent(Some(0)).expect("reads");
    assert_eq!(history.entries.len(), 1);
}

#[test]
fn an_empty_archive_reads_as_empty_rather_than_failing() {
    let (_dir, archive) = archive("empty");
    let history = archive.list_recent(None).expect("reads");
    assert!(history.entries.is_empty());
    assert!(!history.archive_dir.is_empty());
}

#[test]
fn a_corrupt_line_does_not_hide_the_rest() {
    // A half-written line from a killed process must not make every other
    // delivery invisible to the person trying to debug a trigger.
    let (_dir, archive) = archive("corrupt");
    archive
        .record("gmail", "T", "evt-0", "u", &json!({}))
        .expect("records");

    let path = archive.current_day_file();
    let mut contents = fs::read_to_string(&path).expect("reads file");
    contents.push_str("{\"received_at_ms\": tru\n");
    contents.push_str("\n");
    fs::write(&path, contents).expect("writes file");

    archive
        .record("gmail", "T", "evt-1", "u", &json!({}))
        .expect("records");

    let history = archive.list_recent(None).expect("reads");
    let ids: Vec<_> = history
        .entries
        .iter()
        .map(|entry| entry.metadata_id.as_str())
        .collect();
    assert_eq!(ids, ["evt-1", "evt-0"]);
}

#[test]
fn ignores_files_that_are_not_archive_days() {
    let (_dir, archive) = archive("stray");
    archive
        .record("gmail", "T", "evt-0", "u", &json!({}))
        .expect("records");
    fs::write(archive.archive_dir().join("notes.txt"), "not an archive").expect("writes");

    let history = archive.list_recent(None).expect("reads");
    assert_eq!(history.entries.len(), 1);
}

#[test]
fn the_default_limit_is_a_window_not_everything() {
    // The archive can grow without bound; the default has to be a window or a
    // history read eventually returns a user's entire trigger history.
    assert!(DEFAULT_HISTORY_LIMIT > 0);
    assert!(DEFAULT_HISTORY_LIMIT <= 200);
}

#[test]
fn writes_land_in_the_current_day_file() {
    let (_dir, archive) = archive("dayfile");
    archive
        .record("gmail", "T", "evt-0", "u", &json!({}))
        .expect("records");

    let path = archive.current_day_file();
    assert!(path.exists());
    assert_eq!(path.extension().and_then(|e| e.to_str()), Some("jsonl"));

    let contents = fs::read_to_string(&path).expect("reads");
    assert_eq!(contents.lines().count(), 1, "one line per delivery");
    assert!(contents.ends_with('\n'), "every record is a complete line");
}
