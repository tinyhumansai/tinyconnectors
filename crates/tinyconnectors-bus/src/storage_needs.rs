//! Storage resources the `TinyConnectors` module requests from its host.

use std::sync::LazyLock;

use tinystoragedrivers_bus::{CollectionSpec, StorageNeeds};

static PREFERENCE_COLLECTIONS: LazyLock<[CollectionSpec; 1]> = LazyLock::new(|| {
    [CollectionSpec {
        name: "composio-user-scopes".into(),
        indexes: Vec::new(),
        ttl_field: None,
        search: None,
    }]
});

static BLOB_PREFIXES: [&str; 0] = [];
static STREAM_PREFIXES: [&str; 1] = ["triggers/"];
static SECRET_PREFIXES: [&str; 0] = [];

/// The module's requested storage resources; hosts decide the grant.
pub static STORAGE_NEEDS: LazyLock<StorageNeeds<'static>> = LazyLock::new(|| StorageNeeds {
    collections: &PREFERENCE_COLLECTIONS[..],
    blob_prefixes: &BLOB_PREFIXES,
    stream_prefixes: &STREAM_PREFIXES,
    secret_prefixes: &SECRET_PREFIXES,
});

#[cfg(test)]
#[path = "storage_needs_tests.rs"]
mod tests;
