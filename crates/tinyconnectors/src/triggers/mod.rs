//! The archive of webhook deliveries.
//!
//! A trigger fires while nobody is watching — that is the point of it — so the
//! only way to answer "did my Gmail trigger actually fire?" is to have written
//! it down. This module is that record.
//!
//! # Why a file, not a database
//!
//! One append per delivery, read back newest-first, never queried by anything
//! but time. A daily JSONL file does exactly that, is inspectable with `tail`
//! when someone is debugging a trigger that did not fire, and needs no schema
//! migration when a payload shape changes — the payload is opaque JSON.
//!
//! Splitting by day rather than one growing file means a read for the last
//! twenty entries opens one small file instead of seeking the end of a large
//! one, and retention is a matter of deleting old files.
//!
//! # Concurrency
//!
//! Appends take an exclusive file lock. Two deliveries can arrive at once, and
//! interleaved partial writes would corrupt both lines — JSONL's one guarantee
//! is that a line is a record.

mod archive;

pub use archive::{DEFAULT_HISTORY_LIMIT, TriggerArchive};

#[cfg(test)]
mod test;
