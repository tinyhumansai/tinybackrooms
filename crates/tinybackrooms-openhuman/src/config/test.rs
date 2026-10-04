//! Unit tests for the backend configuration.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

#[test]
fn defaults_the_turn_timeout() {
    let config = OpenHumanConfig::new("http://localhost/v1", "key", "model");
    assert_eq!(config.turn_timeout, DEFAULT_TURN_TIMEOUT);
    assert_eq!(config.model, "model");
}

#[test]
fn overrides_the_turn_timeout() {
    let config = OpenHumanConfig::new("u", "k", "m").with_turn_timeout(Duration::from_secs(5));
    assert_eq!(config.turn_timeout, Duration::from_secs(5));
}

#[test]
fn debug_never_prints_the_api_key() {
    let config = OpenHumanConfig::new("http://localhost/v1", "sk-secret-value", "m");
    let shown = format!("{config:?}");
    assert!(!shown.contains("sk-secret-value"), "{shown}");
    assert!(shown.contains("<redacted>"));
}

#[test]
fn the_runtime_config_disables_everything_that_reaches_out() {
    let config = runtime_config();
    assert!(!config.local_ai.runtime_enabled);
    assert!(!config.runtime_python.enabled);
    assert!(!config.memory_tree.spacy_enabled);
    assert_eq!(config.memory_tree.embedding_endpoint, None);
}

#[test]
fn retries_an_empty_response_twice_by_default_and_can_be_changed() {
    let config = OpenHumanConfig::new("http://localhost/v1", "k", "m");
    assert_eq!(config.empty_retries, DEFAULT_EMPTY_RETRIES);
    assert_eq!(config.with_empty_retries(0).empty_retries, 0);
}
