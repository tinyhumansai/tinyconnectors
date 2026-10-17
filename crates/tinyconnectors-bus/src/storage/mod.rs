//! What the module stores, declared as a contract.
//!
//! The host composes every owner's storage schema at boot and derives each
//! module's storage grant from it, so the declaration has to be reachable
//! without linking the module's implementation. It lives here, beside the wire
//! types, as plain data: names, versions, and pure v1 to v2 migration steps.
//! Nothing in this module performs I/O; the host runs the steps.
//!
//! The names and the preference encoding are always available. The schema and
//! the grant need the storage ports and sit behind the `storage` feature, off
//! by default.
//!
//! - `storage_schema` — every collection and stream, plus the steps.
//! - `storage_needs` — the grant generated from that schema.
//! - the name constants and the preference body encoding the document store
//!   in the module crate shares with the steps.

mod names;
#[cfg(feature = "storage")]
mod needs;
mod preference;
#[cfg(feature = "storage")]
mod schema;

pub use names::{
    LEGACY_PREFS_NAMESPACE, LEGACY_STREAM_PREFIX, NON_OBJECT_FIELD, PREFS_NAMESPACE,
    PREFS_SCHEMA_VERSION, STREAM_NAME, STREAM_PREFIX,
};
#[cfg(feature = "storage")]
pub use needs::{StorageNeeds, storage_needs};
pub use preference::{decode_preference, encode_preference};
#[cfg(feature = "storage")]
pub use schema::storage_schema;
