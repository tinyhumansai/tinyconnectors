//! Ingestion record and sync-progress payloads.

use serde::{Deserialize, Serialize};

/// One item pulled out of a connected account.
///
/// The field set is memory's ingestion vocabulary exactly — see the module
/// docs. Anything a connector knows that memory does not ingest belongs on
/// [`ConnectorRecordBatch`], not here.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorRecord {
    /// Stable per-source item id. A dedupe key, not a display value.
    ///
    /// Stable is the load-bearing word: a sync that re-derives this differently
    /// between runs re-ingests everything as new, and the user's memory fills
    /// with duplicates of things they already had.
    pub item_id: String,
    /// Display title.
    #[serde(default)]
    pub title: String,
    /// Item body, already decoded to text.
    ///
    /// Decoding is the connector's job because only it knows what the provider
    /// sent — quoted-printable email, a Notion block tree, a Slack message with
    /// entity escapes. Memory receives text.
    pub content: String,
    /// MIME type of [`Self::content`] when known.
    #[serde(default)]
    pub mime: Option<String>,
    /// Canonical URL back to the item, when it has one.
    #[serde(default)]
    pub url: Option<String>,
    /// Upstream last-modified time in milliseconds since the Unix epoch.
    #[serde(default)]
    pub updated_at_ms: Option<i64>,
    /// Labels carried through from the source.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// One batch of records from one sync run.
///
/// Carries the provenance memory does not ingest but a host needs: which
/// toolkit and connection produced these, and whether there is more to come.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorRecordBatch {
    /// Memory source these records belong to.
    pub source_id: String,
    /// Toolkit slug the records came from, e.g. `"gmail"`.
    pub toolkit: String,
    /// Connection the records were read through.
    ///
    /// A user may hold several connections for one toolkit, and records from
    /// two of them must not merge into one source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    /// The records themselves.
    #[serde(default)]
    pub records: Vec<ConnectorRecord>,
    /// Opaque position to resume from, when more remains.
    ///
    /// Opaque on purpose: it is the provider's own pagination token in most
    /// cases, and a host that parsed it would break the first time a provider
    /// changed its format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// Whether this batch completes the run.
    ///
    /// A host uses this rather than an empty `records` to decide it is done: a
    /// provider can legitimately return an empty page with a cursor still set.
    #[serde(default)]
    pub complete: bool,
}

/// Arguments for running one sync.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConnectorSyncRequest {
    /// Toolkit to sync.
    pub toolkit: String,
    /// Connection to read. `None` uses the toolkit's first active account.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    /// Memory source the records belong to. Defaults to
    /// `<toolkit>:<connection_id>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    /// Most records to produce. `None` takes the module's default.
    ///
    /// A bound, not a target: a run stops at whichever of this, the day's
    /// request budget, and the provider running out comes first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_items: Option<usize>,
    /// Why the run was started, for the log and the status line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// What one sync run produced.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConnectorSyncResponse {
    /// The records to ingest.
    ///
    /// Ingesting them is the caller's job. The module reads a connected
    /// account; the memory engine stores things; neither links the other.
    pub batch: ConnectorRecordBatch,
    /// Where the run got to.
    pub stage: SyncStage,
    /// Pages the provider was asked for.
    #[serde(default)]
    pub pages_read: u32,
    /// Records skipped because they were already ingested and unchanged.
    #[serde(default)]
    pub records_skipped: usize,
    /// Detail worth surfacing — the failure, when the run stopped on one.
    ///
    /// A run that stopped part-way still returns what it read: the caller
    /// should ingest [`Self::batch`] regardless of this field, and call again
    /// while [`ConnectorRecordBatch::complete`] is false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Where a sync run has got to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncStage {
    /// Queued, not yet started.
    #[default]
    Requested,
    /// Reading from the provider.
    Fetching,
    /// Records fetched and held, not yet ingested.
    Stored,
    /// Being written into memory.
    Ingesting,
    /// Finished successfully.
    Completed,
    /// Stopped on an error. See [`SyncEvent::message`].
    Failed,
}

impl SyncStage {
    /// The stable wire name, for a host that renders or logs the stage.
    ///
    /// Spelled out rather than derived from the variant so a rename in Rust
    /// cannot silently change what a UI matches on.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Fetching => "fetching",
            Self::Stored => "stored",
            Self::Ingesting => "ingesting",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    /// Whether the run has stopped, either way.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }
}

/// One progress report from a sync run.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncEvent {
    /// Memory source the run is for.
    pub source_id: String,
    /// Toolkit slug being synced.
    pub toolkit: String,
    /// Connection being read, when the run is scoped to one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    /// Where the run has got to.
    pub stage: SyncStage,
    /// Detail for the stage — the failure, for [`SyncStage::Failed`].
    ///
    /// Must carry no record content and no credential: this is logged and shown
    /// in status output, where a user's email body has no business appearing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
