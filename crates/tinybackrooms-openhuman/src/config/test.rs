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
