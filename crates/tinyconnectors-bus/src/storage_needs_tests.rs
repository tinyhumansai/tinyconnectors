//! Tests for the host-visible storage declaration.

use super::*;

#[test]
fn declares_the_exact_preference_and_trigger_resources() {
    let needs = &*STORAGE_NEEDS;
    assert_eq!(needs.collections.len(), 1);
    let collection = &needs.collections[0];
    assert_eq!(collection.name, "composio-user-scopes");
    assert!(collection.indexes.is_empty());
    assert!(collection.ttl_field.is_none());
    assert!(collection.search.is_none());
    assert!(needs.blob_prefixes.is_empty());
    assert_eq!(needs.stream_prefixes, &["triggers/"]);
    assert!(needs.secret_prefixes.is_empty());
}
