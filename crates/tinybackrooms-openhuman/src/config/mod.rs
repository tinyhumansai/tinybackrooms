//! Configuration for the `OpenHuman` backend.
//!
//! The adapter reads nothing from the environment: a host builds an
//! [`OpenHumanConfig`] and hands it over, so the API crate owns variable names
//! and this crate stays testable.

use std::fmt;
use std::time::Duration;

use openhuman_embed::RuntimeConfig;

/// How long one agent turn may run before it is abandoned.
pub const DEFAULT_TURN_TIMEOUT: Duration = Duration::from_secs(300);

/// Where the models are and how to reach them.
#[derive(Clone, PartialEq, Eq)]
pub struct OpenHumanConfig {
    /// Base URL of an OpenAI-compatible API, such as `https://openrouter.ai/api/v1`.
    pub base_url: String,
    /// The bearer token. `OpenHuman` refuses to send one over plain HTTP to a
    /// non-loopback host.
    pub api_key: String,
    /// The model used when a request names none.
    pub model: String,
    /// The per-turn deadline. The engine links no timer, so this is where a
    /// hung turn is cut off.
    pub turn_timeout: Duration,
}

impl OpenHumanConfig {
    /// A configuration with the default turn timeout.
    #[must_use]
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: model.into(),
            turn_timeout: DEFAULT_TURN_TIMEOUT,
        }
    }

    /// Use `timeout` as the per-turn deadline.
    #[must_use]
    pub fn with_turn_timeout(mut self, timeout: Duration) -> Self {
        self.turn_timeout = timeout;
        self
    }
}

impl fmt::Debug for OpenHumanConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenHumanConfig")
            .field("base_url", &self.base_url)
            .field("api_key", &"<redacted>")
            .field("model", &self.model)
            .field("turn_timeout", &self.turn_timeout)
            .finish()
    }
}

/// The runtime configuration every run boots with: nothing that would reach out
/// or spawn a process. A host that wants more loads its own and builds the
/// runtime itself.
#[must_use]
pub(crate) fn runtime_config() -> RuntimeConfig {
    let mut config = RuntimeConfig::default();
    // Providers answer with native tool calls, not prose-dispatched ones.
    config.agent.tool_dispatcher = "auto".into();
    config.local_ai.runtime_enabled = false;
    config.runtime_python.enabled = false;
    config.memory_tree.spacy_enabled = false;
    config.memory_tree.embedding_endpoint = None;
    config.memory_tree.embedding_model = None;
    config.memory_tree.embedding_strict = false;
    config
}

#[cfg(test)]
mod test;
