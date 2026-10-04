//! Unit tests for the pieces of the conversant that need no runtime.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

#[test]
fn maps_roles_to_the_names_openhuman_seeds_with() {
    assert_eq!(role_name(Role::User), "user");
    assert_eq!(role_name(Role::Assistant), "assistant");
}

#[test]
fn failures_become_conversant_errors_with_context() {
    assert_eq!(
        failure("turn failed", "boom"),
        Error::Conversant("turn failed: boom".into())
    );
}
