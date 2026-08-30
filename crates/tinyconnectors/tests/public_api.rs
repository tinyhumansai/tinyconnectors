//! Integration tests for the public crate surface.
//!
//! These tests link against the crate as a downstream consumer would: they can
//! only use what `src/lib.rs` re-exports. Treat them as the regression suite
//! for the crate's public contract — if a change breaks a test here, it is a
//! breaking change for users.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tinyconnectors::{
    ComposioConnection, Error, client::Transport, names, oauth, tinyconnectors_bus,
};

#[test]
fn the_payload_vocabulary_is_the_contract_crate_s_own_types() {
    // Not a tautology: if this crate ever redefined the payloads rather than
    // re-exporting them, a host linking `tinyconnectors-bus` would be passing
    // structural twins that need a conversion nothing checks.
    fn accepts(_: tinyconnectors_bus::ComposioConnection) {}

    let connection: ComposioConnection = serde_json::from_value(serde_json::json!({
        "id": "c1", "toolkit": "gmail", "status": "ACTIVE"
    }))
    .unwrap();
    accepts(connection);
}

#[test]
fn the_bus_identity_is_available_to_consumers() {
    assert_eq!(names::INTERFACE, "ai.tinyhumans.connectors.Composio");
    assert_eq!(names::OBJECT_PATH, "/ai/tinyhumans/connectors/Composio");
    // Not a count: a count passes through a member being swapped for another,
    // and the point of the table is which members a host may call.
    assert!(names::METHODS.contains(&names::methods::AUTHORIZE));
    assert!(names::METHODS.contains(&names::methods::EXECUTE));
    assert!(names::METHODS.contains(&names::methods::LIST_TRIGGER_HISTORY));
}

#[test]
fn the_oauth_policy_is_available_to_consumers() {
    assert!(oauth::is_meta_oauth_toolkit("instagram"));
    assert!(oauth::is_clearable_oauth_status("PENDING"));
    assert!(!oauth::is_clearable_oauth_status("ACTIVE"));
}

#[test]
fn errors_are_available_to_consumers() {
    let error = Error::Authorize {
        toolkit: "gmail".into(),
        message: "invalid scope".into(),
    };
    assert_eq!(
        error.to_string(),
        "authorize failed for gmail: invalid scope"
    );
}

#[test]
fn the_transport_seam_is_implementable_from_outside_the_crate() {
    // The whole point of the seam: a host supplies its own auth. If `Transport`
    // stopped being nameable and object-safe from outside, this stops building.
    fn assert_object_safe(_: &dyn Transport) {}
    let _ = assert_object_safe;
}

/// The module must load with no configuration at all.
///
/// This is what the release gate does — and what any host loading modules
/// generically does. It was found by that gate failing: the module used to
/// refuse an empty configuration, which meant it could not be verified after
/// publication and could not answer the members that need no credential.
#[tokio::test]
async fn the_built_module_loads_with_an_empty_configuration() {
    use tinybus::broker::Broker;
    use tinybus::module::ModuleHost;
    use tinybus::transport::memory::MemoryBus;

    let module = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/debug")
        .join(format!(
            "{}tinyconnectors{}",
            std::env::consts::DLL_PREFIX,
            std::env::consts::DLL_SUFFIX
        ));
    if !module.exists() {
        // `cargo test` does not guarantee the cdylib is built; the example and
        // the release workflow both cover this against a real artifact.
        return;
    }

    let bus = MemoryBus::new();
    let broker = Broker::new();
    let task = broker.spawn(bus.clone());
    let host = ModuleHost::new(broker);

    let info = host
        .load_file_with_config(&module, serde_json::json!({}))
        .expect("an empty configuration must load");
    assert_eq!(info.name, "tinyconnectors");
    task.abort();
}
