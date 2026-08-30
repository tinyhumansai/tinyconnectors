//! Appending to and reading back the daily JSONL trigger archive.

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};

use fs4::FileExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{ComposioTriggerHistoryEntry, ComposioTriggerHistoryResult, Error, Result};

/// Deliveries returned when a caller does not say how many it wants.
pub const DEFAULT_HISTORY_LIMIT: usize = 50;

const ARCHIVE_SUBDIR: &str = "triggers";
const FILE_EXTENSION: &str = "jsonl";

/// A daily-rotated record of webhook deliveries.
#[derive(Debug, Clone)]
pub struct TriggerArchive {
    archive_dir: PathBuf,
}

impl TriggerArchive {
    /// Open (creating if needed) the archive under `state_dir`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Archive`] when the directory cannot be created.
    pub fn open(state_dir: &Path) -> Result<Self> {
        let archive_dir = state_dir.join(ARCHIVE_SUBDIR);
        fs::create_dir_all(&archive_dir).map_err(|error| Error::Archive {
            path: archive_dir.display().to_string(),
            message: format!("could not create the archive directory: {error}"),
        })?;
        Ok(Self { archive_dir })
    }

    /// The directory the archive writes into.
    #[must_use]
    pub fn archive_dir(&self) -> &Path {
        &self.archive_dir
    }

    /// The file today's deliveries are appended to.
    #[must_use]
    pub fn current_day_file(&self) -> PathBuf {
        self.archive_dir
            .join(format!("{}.{FILE_EXTENSION}", utc_day()))
    }

    /// Append one delivery and return the entry as recorded.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Archive`] if the entry cannot be serialized, or the
    /// file cannot be opened, locked, or written.
    pub fn record(
        &self,
        toolkit: &str,
        trigger: &str,
        metadata_id: &str,
        metadata_uuid: &str,
        payload: &serde_json::Value,
    ) -> Result<ComposioTriggerHistoryEntry> {
        let entry = ComposioTriggerHistoryEntry {
            received_at_ms: now_ms(),
            toolkit: toolkit.to_string(),
            trigger: trigger.to_string(),
            metadata_id: metadata_id.to_string(),
            metadata_uuid: metadata_uuid.to_string(),
            payload: payload.clone(),
        };

        let path = self.current_day_file();
        let failed = |message: String| Error::Archive {
            path: path.display().to_string(),
            message,
        };

        let mut line = serde_json::to_string(&entry)
            .map_err(|error| failed(format!("could not serialize the delivery: {error}")))?;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|error| failed(format!("could not open the archive file: {error}")))?;

        // One `write_all` of one buffer, under an exclusive lock. Two
        // deliveries can arrive at once, and a payload is easily large enough
        // that `O_APPEND` alone does not guarantee the two lines will not
        // interleave — which would corrupt both records, since JSONL's one
        // guarantee is that a line is a record.
        // Fully qualified: `std::fs::File` grew its own inherent `lock` after
        // this workspace's declared MSRV, and an unqualified call would silently
        // switch which one is used depending on the toolchain.
        FileExt::lock(&file)
            .map_err(|error| failed(format!("could not lock the archive file: {error}")))?;
        line.push('\n');
        let written = file.write_all(line.as_bytes()).and_then(|()| file.flush());
        // Unlock even when the write failed: holding the lock would block every
        // later delivery on a failure that has already been reported.
        let unlocked = FileExt::unlock(&file);
        written.map_err(|error| failed(format!("could not append to the archive: {error}")))?;
        unlocked.map_err(|error| failed(format!("could not unlock the archive: {error}")))?;

        tracing::debug!(
            toolkit = %entry.toolkit,
            trigger = %entry.trigger,
            metadata_id = %entry.metadata_id,
            "[connectors][triggers] delivery archived"
        );
        Ok(entry)
    }

    /// The most recent `limit` deliveries, newest first.
    ///
    /// A line that will not parse is skipped rather than failing the read: one
    /// corrupt record — a half-written line from a killed process — must not
    /// hide every other delivery the user is trying to look at.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Archive`] when the archive directory cannot be listed.
    pub fn list_recent(&self, limit: Option<usize>) -> Result<ComposioTriggerHistoryResult> {
        let limit = limit.unwrap_or(DEFAULT_HISTORY_LIMIT).max(1);
        let mut day_files = self.day_files()?;
        // Newest day first: the filename is a sortable ISO date.
        day_files.sort_unstable_by(|left, right| right.cmp(left));

        let mut entries: Vec<ComposioTriggerHistoryEntry> = Vec::new();
        for day_file in day_files {
            if entries.len() >= limit {
                break;
            }
            let Ok(file) = File::open(&day_file) else {
                // A file that vanished between listing and opening is not worth
                // failing the whole read for.
                continue;
            };

            let mut day_entries: Vec<ComposioTriggerHistoryEntry> = BufReader::new(file)
                .lines()
                .map_while(std::result::Result::ok)
                .filter(|line| !line.trim().is_empty())
                .filter_map(|line| serde_json::from_str(&line).ok())
                .collect();

            // Within a day the file is append-ordered, so newest is last.
            day_entries.reverse();
            entries.extend(day_entries);
        }

        entries.truncate(limit);
        Ok(ComposioTriggerHistoryResult {
            archive_dir: self.archive_dir.display().to_string(),
            current_day_file: self.current_day_file().display().to_string(),
            entries,
        })
    }

    fn day_files(&self) -> Result<Vec<PathBuf>> {
        let read_dir = fs::read_dir(&self.archive_dir).map_err(|error| Error::Archive {
            path: self.archive_dir.display().to_string(),
            message: format!("could not list the archive directory: {error}"),
        })?;

        Ok(read_dir
            .filter_map(std::result::Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == FILE_EXTENSION)
            })
            .collect())
    }
}

/// Milliseconds since the Unix epoch.
///
/// A clock set before 1970 yields zero rather than panicking: a wrong timestamp
/// on an archived delivery is a great deal better than dropping the delivery.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

/// Today's UTC date as `YYYY-MM-DD`.
///
/// UTC because the filename orders the archive, and a local-time boundary would
/// reorder files when a machine changes timezone.
fn utc_day() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}
