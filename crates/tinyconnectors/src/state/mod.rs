//! Where the module keeps sync state.
//!
//! A provider needs to remember where it got to, what it has already seen, and
//! how much of today's request budget is left. [`tinyconnectors_sync`] defines
//! that as a two-method key-value seam and deliberately does not implement one.
//! This module supplies the module's own.
//!
//! # Why the module owns this now
//!
//! These rows used to live in the memory engine's key-value tier, because that
//! is where the sync pipelines lived. After the migration they are the
//! *module's* cursors, for its own resumption, and nothing else reads them —
//! so keeping them in memory would mean a bus round-trip to memory on every
//! page of every sync, for state memory has no interest in.
//!
//! # Why a file per key
//!
//! One connection's state is read at the start of a run and written at the end,
//! and runs for different connections are independent. A file per key means two
//! concurrent syncs never contend, a corrupt write damages one connection's
//! cursor rather than every connection's, and the whole thing is inspectable.

mod file_store;

pub use file_store::FileStateStore;

#[cfg(test)]
mod test;
