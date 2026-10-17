//! What the module needs the host to grant on its storage bus.
//!
//! The module reaches storage only through the host's storage bus, and the host
//! decides per module which collections and stream names it may touch. A grant
//! that is narrower than this list makes the module's calls fail closed. The
//! list is generated from [`storage_schema`](super::storage_schema), the
//! module's own declaration, so the two cannot drift: every item declared with
//! `ScopeClass::Module` is granted, nothing else is. Only the host declares
//! collections (`Registry::prepare`); the module never creates one.

use tinystoragedrivers_core::schema::{Item, Schema, ScopeClass};

use super::schema::storage_schema;

/// Collections and stream prefixes the module reads and writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageNeeds {
    /// Document collections: one document per toolkit, body the preference.
    pub collections: Vec<String>,
    /// Stream-name prefixes, each a declared stream and a slash: one stream per
    /// UTC day, `connectors.triggers/<YYYY-MM-DD>`.
    pub stream_prefixes: Vec<String>,
}

/// The grant the host gives this module, generated from [`storage_schema`](super::storage_schema).
#[must_use]
pub fn storage_needs() -> StorageNeeds {
    needs_of(storage_schema())
}

/// The module-scoped collections and stream prefixes `schema` declares.
fn needs_of(schema: Schema) -> StorageNeeds {
    let mut needs = StorageNeeds {
        collections: Vec::new(),
        stream_prefixes: Vec::new(),
    };
    for item in schema.items {
        if item.scope() != ScopeClass::Module {
            continue;
        }
        match item {
            Item::Collection { name, .. } => needs.collections.push(name),
            Item::Stream { name, .. } => needs.stream_prefixes.push(format!("{name}/")),
            Item::Blobs { .. } | Item::Secrets { .. } => {}
        }
    }
    needs
}

#[cfg(test)]
#[path = "needs_tests.rs"]
mod tests;
