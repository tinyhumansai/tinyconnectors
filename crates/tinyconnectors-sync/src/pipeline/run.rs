//! The sync loop.

use tinyconnectors_bus::{ConnectorRecord, ConnectorRecordBatch, SyncStage};

use crate::Result;
use crate::provider::{ConnectorProvider, ProviderContext, SyncReason};
use crate::state::SyncState;

/// One page as a provider read it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderPage {
    /// Records on this page, in the order the provider returned them.
    pub records: Vec<ConnectorRecord>,
    /// Version per record id, for sources that report one.
    ///
    /// Separate from the records because most providers have no versions, and
    /// putting an always-`None` field on every record would be dead weight on
    /// the wire — [`ConnectorRecord`] is the shape memory ingests.
    pub versions: Vec<(String, String)>,
    /// Where the next page starts, or `None` when this is the last.
    pub next_cursor: Option<String>,
}

/// What a run did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncOutcome {
    /// The records to ingest.
    pub batch: ConnectorRecordBatch,
    /// Where the run got to.
    pub stage: SyncStage,
    /// Why the run started.
    pub reason: SyncReason,
    /// Pages the provider was asked for.
    pub pages_read: u32,
    /// Records skipped because they were already ingested and unchanged.
    pub records_skipped: usize,
    /// Detail worth surfacing — the failure, when the run stopped on one.
    ///
    /// Carries no record content and no credential: it is logged and shown in
    /// status output, where a user's email body has no business appearing.
    pub message: Option<String>,
}

/// Run one sync for `provider`.
///
/// Loads the connection's cursor, pages until a limit stops it, drops records
/// already ingested, and saves the cursor.
///
/// A provider failure part-way through is **not** an error: the pages already
/// read are real, and the outcome reports [`SyncStage::Failed`] with what was
/// read and why it stopped. Discarding them would mean a connection failing on
/// its fifth page never ingests its first four.
///
/// # Errors
///
/// Returns [`crate::Error::Store`] or [`crate::Error::Decode`] when the sync
/// state cannot be read — a run that cannot tell where it got to must not start
/// over from the beginning and re-ingest a user's history.
pub async fn run_sync(
    provider: &dyn ConnectorProvider,
    context: &ProviderContext,
    reason: SyncReason,
) -> Result<SyncOutcome> {
    let mut state = SyncState::load(
        context.state.as_ref(),
        &context.toolkit,
        &context.connection_id,
    )
    .await?;

    let mut outcome = SyncOutcome {
        batch: ConnectorRecordBatch {
            source_id: context.source_id.clone(),
            toolkit: context.toolkit.clone(),
            connection_id: Some(context.connection_id.clone()),
            records: Vec::new(),
            cursor: state.cursor.clone(),
            complete: false,
        },
        stage: SyncStage::Fetching,
        reason,
        pages_read: 0,
        records_skipped: 0,
        message: None,
    };

    if state.budget_exhausted() {
        // Not a failure: the budget did its job. Reporting it as one would put
        // a red status on a connection that is working exactly as configured.
        outcome.stage = SyncStage::Completed;
        outcome.message = Some("today's provider request budget is spent".to_string());
        return Ok(outcome);
    }

    let mut cursor = state.cursor.clone();
    loop {
        if outcome.batch.records.len() >= context.limits.max_items {
            // Stopped by the item limit, so there is more to read: the cursor
            // is saved and the batch is deliberately not complete.
            break;
        }
        if state.budget_exhausted() {
            outcome.message = Some("today's provider request budget is spent".to_string());
            break;
        }

        let page = match provider.fetch_page(context, cursor.as_deref()).await {
            Ok(page) => page,
            Err(error) => {
                // Keep what was read. A connection failing on its fifth page
                // must still ingest its first four.
                outcome.stage = SyncStage::Failed;
                outcome.message = Some(error.to_string());
                break;
            }
        };

        outcome.pages_read += 1;
        state.record_action(1, 0.0);

        let versions: std::collections::HashMap<&str, &str> = page
            .versions
            .iter()
            .map(|(id, version)| (id.as_str(), version.as_str()))
            .collect();

        for record in page.records {
            let version = versions.get(record.item_id.as_str()).copied();
            if !state.needs_ingest(&record.item_id, version) {
                outcome.records_skipped += 1;
                continue;
            }
            state.mark_synced(record.item_id.clone(), version);
            outcome.batch.records.push(record);

            if outcome.batch.records.len() >= context.limits.max_items {
                break;
            }
        }

        cursor = page.next_cursor;
        match cursor.as_deref() {
            Some(next) => state.advance_cursor(next),
            None => {
                // The provider has no more to give: this is the only path that
                // completes a run.
                outcome.stage = SyncStage::Completed;
                outcome.batch.complete = true;
                break;
            }
        }
    }

    outcome.batch.cursor = state.cursor.clone();
    if outcome.stage == SyncStage::Fetching {
        // Stopped by a limit rather than by the provider or a failure.
        outcome.stage = SyncStage::Completed;
    }
    state.set_last_sync_at_ms(now_ms());

    // Saved even after a failure: the pages already read are real, and throwing
    // the cursor away would re-read them on every attempt.
    state.save(context.state.as_ref()).await?;

    tracing::info!(
        toolkit = %context.toolkit,
        connection_id = %context.connection_id,
        reason = reason.as_str(),
        stage = outcome.stage.as_str(),
        pages = outcome.pages_read,
        ingested = outcome.batch.records.len(),
        skipped = outcome.records_skipped,
        "[connectors][sync] run finished"
    );
    Ok(outcome)
}

/// Milliseconds since the Unix epoch.
///
/// A clock set before 1970 yields zero rather than panicking: a wrong
/// last-synced stamp is much better than a failed sync.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}
