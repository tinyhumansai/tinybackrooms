//! Request and reply types for the [`Conversant`](super::Conversant) port.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Who authored a message, from the point of view of the agent being asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// The other party in the conversation, or the engine's framing.
    User,
    /// The agent's own earlier turns.
    Assistant,
}

/// One message in an agent's history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// Who wrote it.
    pub role: Role,
    /// What it says.
    pub content: String,
}

impl Message {
    /// A message from the other party.
    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
        }
    }

    /// A message the agent itself wrote.
    #[must_use]
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
        }
    }
}

/// Which seat of the engine is asking, so a backend can keep per-seat state
/// (such as a memory namespace or a session) apart.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Seat {
    /// The variation this call belongs to, such as `v0`.
    pub run_id: String,
    /// The role within it, such as `agent_a` or `formatter`.
    pub role: String,
}

/// Ask one agent for its next free-text turn.
#[derive(Debug, Clone, PartialEq)]
pub struct TurnRequest {
    /// The seat asking.
    pub seat: Seat,
    /// The model to use; `None` means the backend default.
    pub model: Option<String>,
    /// Sampling temperature.
    pub temperature: f32,
    /// The system prompt.
    pub system: String,
    /// The history, ending with the message to answer.
    pub messages: Vec<Message>,
}

/// Ask an agent to answer by calling a typed tool.
#[derive(Debug, Clone, PartialEq)]
pub struct StructuredRequest {
    /// The seat asking.
    pub seat: Seat,
    /// The model to use; `None` means the backend default.
    pub model: Option<String>,
    /// The system prompt.
    pub system: String,
    /// The history, ending with the message to answer.
    pub messages: Vec<Message>,
    /// The tool's name, which the agent is told to call.
    pub tool: String,
    /// JSON Schema the tool's arguments must follow.
    pub schema: Value,
}

/// A free-text answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnReply {
    /// What the agent said.
    pub text: String,
    /// What the call cost.
    pub usage: Usage,
}

/// A typed answer: the arguments the agent passed to the tool.
#[derive(Debug, Clone, PartialEq)]
pub struct StructuredReply {
    /// The tool arguments, not yet validated against the schema.
    pub value: Value,
    /// What the call cost.
    pub usage: Usage,
}

/// Token accounting for a call or a whole run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// Prompt tokens, when the backend reports them.
    pub input_tokens: u64,
    /// Completion tokens, when the backend reports them.
    pub output_tokens: u64,
    /// Backend calls made.
    pub calls: u64,
}

impl Usage {
    /// Add another call's usage to this total.
    pub fn add(&mut self, other: Usage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.calls += other.calls;
    }
}
