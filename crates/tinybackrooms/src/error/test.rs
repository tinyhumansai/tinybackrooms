//! Unit tests for the crate-wide error type.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

#[test]
fn renders_messages_without_trailing_punctuation() {
    let all = [
        Error::EmptyBrief,
        Error::NoVariations,
        Error::TooManyVariations { max: 64 },
        Error::NoGenerations,
        Error::NoItems,
        Error::InvalidWeights,
        Error::InvalidTopK,
        Error::InvalidSchema("bad".into()),
        Error::Conversant("down".into()),
        Error::Memory("full".into()),
        Error::NoValidItems,
    ];
    for error in all {
        let message = error.to_string();
        assert_ne!(message.len(), 0);
        assert!(!message.ends_with('.'), "{message}");
    }
}

#[test]
fn carries_the_limit_in_the_message() {
    assert_eq!(
        Error::TooManyVariations { max: 64 }.to_string(),
        "variations must be at most 64"
    );
}

#[test]
fn is_a_standard_error() {
    fn assert_error<E: std::error::Error>(_: &E) {}
    assert_error(&Error::EmptyBrief);
}
