//! A preference store backed by one JSON file per toolkit.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tinyconnectors_sync::{Error as SyncError, PREFS_NAMESPACE, PrefsStore, Result as SyncResult};

/// Scope preferences persisted under a directory the host named.
#[derive(Debug, Clone)]
pub struct FilePrefsStore {
    root: PathBuf,
}

impl FilePrefsStore {
    /// Store preferences under `state_dir`.
    #[must_use]
    pub fn new(state_dir: &Path) -> Self {
        Self {
            root: state_dir.to_path_buf(),
        }
    }

    /// The file one key lives in.
    ///
    /// The key becomes a path segment, so it is sanitized: a key containing `/`
    /// or `..` would otherwise write outside the state directory entirely. Keys
    /// are toolkit slugs, which arrive from a config file, a UI field and a
    /// backend envelope.
    fn path_for(&self, key: &str) -> PathBuf {
        self.root
            .join(sanitize(PREFS_NAMESPACE))
            .join(format!("{}.json", sanitize(key)))
    }
}

/// Replace anything that is not plainly safe in a filename.
///
/// An allowlist, not a denylist: the set of characters that mean something to a
/// path is longer than it looks, and differs between platforms.
fn sanitize(segment: &str) -> String {
    let cleaned: String = segment
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect();

    // `.` and `..` survive the filter above and both traverse.
    if cleaned.is_empty() || cleaned.chars().all(|character| character == '.') {
        "_".to_string()
    } else {
        cleaned
    }
}

#[async_trait]
impl PrefsStore for FilePrefsStore {
    async fn get(&self, key: &str) -> SyncResult<Option<serde_json::Value>> {
        let path = self.path_for(key);
        let failed = |message: String| SyncError::Store {
            key: key.to_string(),
            message,
        };

        let contents = match tokio::fs::read_to_string(&path).await {
            Ok(contents) => contents,
            // A toolkit the user never configured is the normal case, not a
            // failure.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(failed(format!(
                    "could not read {}: {error}",
                    path.display()
                )));
            }
        };

        serde_json::from_str(&contents)
            .map(Some)
            .map_err(|error| failed(format!("{} is not valid JSON: {error}", path.display())))
    }

    async fn set(&self, key: &str, value: &serde_json::Value) -> SyncResult<()> {
        let path = self.path_for(key);
        let failed = |message: String| SyncError::Store {
            key: key.to_string(),
            message,
        };

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|error| {
                failed(format!("could not create {}: {error}", parent.display()))
            })?;
        }

        let serialized = serde_json::to_vec(value)
            .map_err(|error| failed(format!("could not serialize the value: {error}")))?;

        // Write beside, then rename. A process killed mid-write would otherwise
        // leave a truncated file, and a preference that will not parse is an
        // error until someone deletes it by hand.
        let temporary = path.with_extension("json.tmp");
        tokio::fs::write(&temporary, &serialized)
            .await
            .map_err(|error| failed(format!("could not write {}: {error}", temporary.display())))?;
        tokio::fs::rename(&temporary, &path)
            .await
            .map_err(|error| failed(format!("could not replace {}: {error}", path.display())))?;

        Ok(())
    }
}
