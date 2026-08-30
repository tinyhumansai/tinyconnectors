//! What a connector sync produces, on its way to memory.
//!
//! This is the second thing that crosses the bus, and it goes the other
//! direction from everything in [`crate::composio`]. Those are answers to
//! questions a host asks. These are what a sync *emits*: the records pulled out
//! of a connected account, handed to the host, and written into memory over
//! memory's own bus API.
//!
//! # Why the connector does not write memory itself
//!
//! It used to. The sync pipelines lived inside `tinymemory` and called the
//! memory store directly, which is why they could not move without taking half
//! of memory with them. Returning records instead cuts that: a sync knows how
//! to talk to Gmail, and memory knows how to store things, and neither has to
//! link the other. The host owns the join.
//!
//! # Why the record is exactly the shape memory ingests
//!
//! [`ConnectorRecord`]'s fields and wire names match the item type memory's
//! ingestion API already accepts, field for field. That is deliberate and
//! tested: a near-miss shape would mean a translation step at the join, and a
//! translation nothing checks is where fields quietly stop arriving. Provenance
//! that memory does not ingest — which toolkit, which connection — lives on
//! [`ConnectorRecordBatch`] instead, because it is a property of the sync run
//! rather than of any one record.

mod types;

pub use types::{ConnectorRecord, ConnectorRecordBatch, SyncEvent, SyncStage};

#[cfg(test)]
mod test;
