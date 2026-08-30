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
    assert_eq!(names::METHODS.len(), 6);
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
