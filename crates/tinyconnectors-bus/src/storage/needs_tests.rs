//! Unit tests for the grant generated from a schema.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tinystoragedrivers_core::schema::{Owner, Schema, ScopeClass};

use super::needs_of;

#[test]
fn only_module_scoped_collections_and_streams_are_granted() {
    let schema = Schema::new(Owner::new("demo").unwrap())
        .collection("demo.granted", 1, ScopeClass::Module)
        .collection("demo.tenant_wide", 1, ScopeClass::Tenant)
        .stream("demo.log", ScopeClass::Module)
        .stream("demo.shared_log", ScopeClass::Agent)
        .blobs("demo/files", ScopeClass::Module)
        .secrets("demo/secrets", ScopeClass::Module);

    let needs = needs_of(schema);

    assert_eq!(needs.collections, ["demo.granted"]);
    assert_eq!(needs.stream_prefixes, ["demo.log/"]);
}
