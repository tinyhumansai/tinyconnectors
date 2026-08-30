//! Unit tests for the bus name table.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{INTERFACE, METHODS, OBJECT_PATH, methods};

#[test]
fn the_object_path_is_the_interface_in_path_form() {
    let expected = format!("/{}", INTERFACE.replace('.', "/"));
    assert_eq!(OBJECT_PATH, expected);
}

#[test]
fn every_member_is_listed_exactly_once() {
    let mut sorted = METHODS.to_vec();
    sorted.sort_unstable();
    let mut deduplicated = sorted.clone();
    deduplicated.dedup();
    assert_eq!(sorted, deduplicated);
}

#[test]
fn the_method_table_holds_the_declared_members() {
    // Spelled out rather than counted: the point of the table is the exact
    // dispatch order, and a length check would pass through a reordering.
    assert_eq!(
        METHODS,
        [
            methods::LIST_TOOLKITS,
            methods::LIST_CONNECTIONS,
            methods::AUTHORIZE,
            methods::DELETE_CONNECTION,
            methods::LIST_TOOLS,
            methods::SYNC,
            methods::GET_USER_SCOPES,
            methods::SET_USER_SCOPES,
            methods::EXECUTE,
            methods::LIST_CAPABILITIES,
            methods::LIST_AGENT_READY_TOOLKITS,
            methods::GET_USER_PROFILE,
            methods::REFRESH_ALL_IDENTITIES,
            methods::LIST_GITHUB_REPOS,
            methods::LIST_AVAILABLE_TRIGGERS,
            methods::LIST_TRIGGERS,
            methods::CREATE_TRIGGER,
            methods::ENABLE_TRIGGER,
            methods::DISABLE_TRIGGER,
            methods::LIST_TRIGGER_HISTORY,
        ]
    );
}

#[test]
fn every_member_name_is_pascal_case() {
    for method in METHODS {
        let first = method.chars().next().expect("member name is not empty");
        assert!(
            first.is_ascii_uppercase(),
            "member {method:?} must start uppercase"
        );
        assert!(
            method.chars().all(|c| c.is_ascii_alphanumeric()),
            "member {method:?} must be alphanumeric"
        );
    }
}

#[test]
fn no_member_name_is_empty() {
    assert!(METHODS.iter().all(|method| !method.is_empty()));
}
