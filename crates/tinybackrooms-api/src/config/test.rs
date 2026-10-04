//! Unit tests for configuration parsing.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

fn lookup<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |key| {
        pairs
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| (*v).to_owned())
    }
}

#[test]
fn defaults_to_loopback_and_the_demo_backend() {
    let config = Config::from_lookup(lookup(&[])).unwrap();
    assert_eq!(config.addr, "127.0.0.1:8080".parse().unwrap());
    assert_eq!(config.backend, BackendKind::Demo);
}

#[test]
fn reads_the_address_and_backend() {
    let config = Config::from_lookup(lookup(&[
        ("TINYBACKROOMS_ADDR", "0.0.0.0:9000"),
        ("TINYBACKROOMS_BACKEND", "demo"),
    ]))
    .unwrap();
    assert_eq!(config.addr.port(), 9000);
}

#[test]
fn rejects_an_unparseable_address() {
    let err = Config::from_lookup(lookup(&[("TINYBACKROOMS_ADDR", "nowhere")])).unwrap_err();
    assert_eq!(err, ConfigError::InvalidAddr("nowhere".into()));
    assert_eq!(err.to_string(), "invalid listen address \"nowhere\"");
}

#[test]
fn rejects_an_unknown_backend() {
    let err = Config::from_lookup(lookup(&[("TINYBACKROOMS_BACKEND", "magic")])).unwrap_err();
    assert_eq!(err, ConfigError::UnknownBackend("magic".into()));
}

#[test]
fn from_env_reads_the_process_environment_without_failing_on_defaults() {
    // The variables are unset in the test environment, so this must hit the defaults.
    if std::env::var_os("TINYBACKROOMS_ADDR").is_none()
        && std::env::var_os("TINYBACKROOMS_BACKEND").is_none()
    {
        assert_eq!(Config::from_env().unwrap().backend, BackendKind::Demo);
    }
}

fn openhuman(extra: &[(&'static str, &'static str)]) -> Result<Config, ConfigError> {
    let mut pairs = vec![("TINYBACKROOMS_BACKEND", "openhuman")];
    pairs.extend_from_slice(extra);
    Config::from_lookup(lookup(&pairs))
}

#[test]
fn the_openhuman_backend_needs_an_api_key() {
    assert_eq!(
        openhuman(&[]).unwrap_err(),
        ConfigError::MissingVariable("TINYBACKROOMS_OPENHUMAN_API_KEY")
    );
    assert!(openhuman(&[("TINYBACKROOMS_OPENHUMAN_API_KEY", "")]).is_err());
}

#[test]
fn the_openhuman_backend_has_defaults() {
    let config = openhuman(&[("TINYBACKROOMS_OPENHUMAN_API_KEY", "k")]).unwrap();
    let BackendKind::OpenHuman(settings) = config.backend else {
        panic!("wrong backend")
    };
    assert_eq!(settings.base_url, "https://openrouter.ai/api/v1");
    assert_eq!(settings.model, "openai/gpt-oss-120b:nitro");
    assert_eq!(settings.turn_timeout_secs, 300);
    assert_eq!(settings.api_key, "k");
}

#[test]
fn the_openhuman_backend_reads_every_variable() {
    let config = openhuman(&[
        ("TINYBACKROOMS_OPENHUMAN_API_KEY", "k"),
        ("TINYBACKROOMS_OPENHUMAN_BASE_URL", "http://localhost:9/v1"),
        ("TINYBACKROOMS_OPENHUMAN_MODEL", "m"),
        ("TINYBACKROOMS_OPENHUMAN_TURN_TIMEOUT_SECS", "42"),
    ])
    .unwrap();
    let BackendKind::OpenHuman(settings) = config.backend else {
        panic!("wrong backend")
    };
    assert_eq!(
        (
            settings.base_url.as_str(),
            settings.model.as_str(),
            settings.turn_timeout_secs
        ),
        ("http://localhost:9/v1", "m", 42)
    );
}

#[test]
fn rejects_a_bad_turn_timeout() {
    for bad in ["soon", "0", "-3"] {
        let err = openhuman(&[
            ("TINYBACKROOMS_OPENHUMAN_API_KEY", "k"),
            ("TINYBACKROOMS_OPENHUMAN_TURN_TIMEOUT_SECS", bad),
        ])
        .unwrap_err();
        assert_eq!(err, ConfigError::InvalidTimeout(bad.into()));
    }
}

#[test]
fn debug_never_prints_the_api_key() {
    let config = openhuman(&[("TINYBACKROOMS_OPENHUMAN_API_KEY", "sk-very-secret")]).unwrap();
    let shown = format!("{config:?}");
    assert!(!shown.contains("sk-very-secret"), "{shown}");
    assert!(shown.contains("<redacted>"));
}

#[test]
fn error_messages_name_the_variable() {
    let message = ConfigError::MissingVariable("TINYBACKROOMS_OPENHUMAN_API_KEY").to_string();
    assert!(message.starts_with("TINYBACKROOMS_OPENHUMAN_API_KEY must be set"));
    assert_eq!(
        ConfigError::InvalidTimeout("x".into()).to_string(),
        "invalid turn timeout \"x\""
    );
}
