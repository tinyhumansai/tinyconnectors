//! The archive of webhook deliveries.
//!
//! A trigger fires while nobody is watching — that is the point of it — so the
//! only way to answer "did my Gmail trigger actually fire?" is to have written
//! it down. This module is that record.
//!
//! # Why a stream
//!
//! One append per delivery, read back newest-first, never queried by anything
//! but time. A daily stream on a `tinystoragedrivers` stream port does exactly
//! that: the stream name is `connectors.triggers/<YYYY-MM-DD>`, each record is one
//! delivery, and the payload is opaque JSON so a payload shape change needs no
//! migration.
//!
//! Splitting by day rather than one growing stream means a read for the last
//! twenty entries touches one small stream instead of the tail of a large one,
//! and retention is a matter of deleting old streams.
//!
//! # Concurrency
//!
//! The port assigns each append the next dense offset atomically, so two
//! deliveries arriving at once are two whole records; no lock is needed here.

mod archive;

pub use archive::{DEFAULT_HISTORY_LIMIT, TriggerArchive};

#[cfg(test)]
#[path = "mod_tests.rs"]
mod test;
