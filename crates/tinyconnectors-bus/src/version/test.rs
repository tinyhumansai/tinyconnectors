//! Unit tests for the contract version and its bind rule.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{CONTRACT_VERSION, binds, is_compatible};

#[test]
fn the_shipped_contract_version_is_pinned() {
    assert_eq!(CONTRACT_VERSION, (1, 4));
}

#[test]
fn the_contract_binds_to_itself() {
    assert!(is_compatible(CONTRACT_VERSION));
}

#[test]
fn a_newer_minor_on_the_module_side_binds() {
    // Relative to the shipped version, not literal: a literal minor stops
    // being "newer" the moment the contract gains a member, which is a test
    // failure that says nothing about the rule it is checking.
    let (major, minor) = CONTRACT_VERSION;
    assert!(is_compatible((major, minor + 1)));
    assert!(is_compatible((major, minor + 96)));
}

#[test]
fn an_older_minor_on_the_module_side_is_rejected() {
    // A host built against 1.4 cannot call a 1.2 module: the members it names
    // may not be served.
    assert!(!binds((1, 4), (1, 2)));
    assert!(binds((1, 4), (1, 4)));
}

#[test]
fn a_different_major_is_rejected() {
    assert!(!is_compatible((0, 0)));
    assert!(!is_compatible((2, 0)));
    assert!(!is_compatible((2, 97)));
}
