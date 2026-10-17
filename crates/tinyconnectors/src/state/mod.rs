//! Where the module keeps the user's scope preferences.
//!
//! [`tinyconnectors_sync`] defines the preference store as a two-method
//! key-value seam ([`tinyconnectors_sync::PrefsStore`]) and deliberately does
//! not implement one. This module supplies the module's own.
//!
//! # Why a document per key
//!
//! A preference is read on every `Execute` and `ListTools` that applies user
//! scopes and written when the user changes it. A document per key means a
//! corrupt write damages one toolkit's preference rather than all of them. The
//! documents live in the [`tinyconnectors_sync::PREFS_NAMESPACE`] collection of
//! a `tinystoragedrivers` document port the host provides; this crate depends on
//! the ports only and never chooses a backend.

mod document_store;

pub use document_store::DocumentPrefsStore;
// The schema and the grant are part of the contract; re-exported so existing
// paths keep working. There is one definition, in `tinyconnectors-bus`.
pub use tinyconnectors_bus::storage::{StorageNeeds, storage_needs, storage_schema};

#[cfg(test)]
#[path = "mod_tests.rs"]
mod test;

#[cfg(test)]
#[path = "migrated_reads_tests.rs"]
mod migrated_reads;
