//! Server configuration, read from the environment.
//!
//! | Variable | Default | Meaning |
//! | --- | --- | --- |
//! | `TINYBACKROOMS_ADDR` | `127.0.0.1:8080` | Socket address to listen on. |
//! | `TINYBACKROOMS_BACKEND` | `demo` | `demo` fabricates schema-valid output without credentials; `openhuman` runs `OpenHuman` agents (needs the `openhuman` feature). |
//! | `TINYBACKROOMS_OPENHUMAN_API_KEY` | none, required for `openhuman` | Bearer token for the model provider. |
//! | `TINYBACKROOMS_OPENHUMAN_BASE_URL` | `https://openrouter.ai/api/v1` | OpenAI-compatible API base URL. |
//! | `TINYBACKROOMS_OPENHUMAN_MODEL` | `openai/gpt-oss-120b:nitro` | Model used when a request names none. |
//! | `TINYBACKROOMS_OPENHUMAN_TURN_TIMEOUT_SECS` | `300` | Per-turn deadline. |
//!
//! Parsing goes through [`Config::from_lookup`] so tests never touch the
//! process environment.

use std::fmt;
use std::net::SocketAddr;

/// The address used when `TINYBACKROOMS_ADDR` is unset.
const DEFAULT_ADDR: &str = "127.0.0.1:8080";
const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";
const DEFAULT_MODEL: &str = "openai/gpt-oss-120b:nitro";
const DEFAULT_TURN_TIMEOUT_SECS: u64 = 300;

/// Which model backend serves requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendKind {
    /// The credential-free [`DemoConversant`](crate::DemoConversant).
    Demo,
    /// `OpenHuman` agents talking to an OpenAI-compatible provider.
    OpenHuman(OpenHumanSettings),
}

/// Where the `OpenHuman` backend's models are.
#[derive(Clone, PartialEq, Eq)]
pub struct OpenHumanSettings {
    /// Base URL of an OpenAI-compatible API.
    pub base_url: String,
    /// The bearer token.
    pub api_key: String,
    /// The default model.
    pub model: String,
    /// Per-turn deadline, in seconds.
    pub turn_timeout_secs: u64,
}

impl fmt::Debug for OpenHumanSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenHumanSettings")
            .field("base_url", &self.base_url)
            .field("api_key", &"<redacted>")
            .field("model", &self.model)
            .field("turn_timeout_secs", &self.turn_timeout_secs)
            .finish()
    }
}

/// Everything the server needs to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Where to listen.
    pub addr: SocketAddr,
    /// Which backend answers.
    pub backend: BackendKind,
}

/// A configuration value that could not be used.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConfigError {
    /// `TINYBACKROOMS_ADDR` was not a socket address.
    #[error("invalid listen address {0:?}")]
    InvalidAddr(String),
    /// `TINYBACKROOMS_BACKEND` named a backend that does not exist.
    #[error("unknown backend {0:?}")]
    UnknownBackend(String),
    /// The `openhuman` backend needs a variable that was not set.
    #[error("{0} must be set for the openhuman backend")]
    MissingVariable(&'static str),
    /// `TINYBACKROOMS_OPENHUMAN_TURN_TIMEOUT_SECS` was not a positive integer.
    #[error("invalid turn timeout {0:?}")]
    InvalidTimeout(String),
}

impl Config {
    /// Read the configuration from the process environment.
    ///
    /// # Errors
    ///
    /// Returns a [`ConfigError`] when a variable is set to an unusable value or
    /// a required one is missing.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Read the configuration through `lookup`, which maps a variable name to
    /// its value.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::InvalidAddr`], [`ConfigError::UnknownBackend`],
    /// [`ConfigError::MissingVariable`] or [`ConfigError::InvalidTimeout`] for
    /// unusable values.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let addr = lookup("TINYBACKROOMS_ADDR").unwrap_or_else(|| DEFAULT_ADDR.to_owned());
        let addr = addr.parse().map_err(|_| ConfigError::InvalidAddr(addr))?;
        let backend = match lookup("TINYBACKROOMS_BACKEND").as_deref() {
            None | Some("demo") => BackendKind::Demo,
            Some("openhuman") => BackendKind::OpenHuman(openhuman_settings(&lookup)?),
            Some(other) => return Err(ConfigError::UnknownBackend(other.to_owned())),
        };
        Ok(Self { addr, backend })
    }
}

fn openhuman_settings(
    lookup: &impl Fn(&str) -> Option<String>,
) -> Result<OpenHumanSettings, ConfigError> {
    const KEY: &str = "TINYBACKROOMS_OPENHUMAN_API_KEY";
    let api_key = lookup(KEY)
        .filter(|k| !k.is_empty())
        .ok_or(ConfigError::MissingVariable(KEY))?;
    let turn_timeout_secs = match lookup("TINYBACKROOMS_OPENHUMAN_TURN_TIMEOUT_SECS") {
        None => DEFAULT_TURN_TIMEOUT_SECS,
        Some(text) => text
            .parse::<u64>()
            .ok()
            .filter(|secs| *secs > 0)
            .ok_or(ConfigError::InvalidTimeout(text))?,
    };
    Ok(OpenHumanSettings {
        base_url: lookup("TINYBACKROOMS_OPENHUMAN_BASE_URL")
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_owned()),
        api_key,
        model: lookup("TINYBACKROOMS_OPENHUMAN_MODEL").unwrap_or_else(|| DEFAULT_MODEL.to_owned()),
        turn_timeout_secs,
    })
}

#[cfg(test)]
mod test;
