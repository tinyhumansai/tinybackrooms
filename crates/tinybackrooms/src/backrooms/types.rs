//! Types describing a finished conversation.

use serde::{Deserialize, Serialize};

/// One turn of a conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    /// Who spoke, such as `agent_a`.
    pub agent: String,
    /// What they said.
    pub text: String,
}

/// Why a conversation ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// It ran for the configured number of turns.
    Generations,
    /// An agent typed the `^C^C` interrupt.
    Interrupt,
    /// The wall-clock budget ran out before the next turn.
    WallClock,
}

/// A whole conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transcript {
    /// The variation it belongs to, such as `v0`.
    pub run_id: String,
    /// Every turn, in order.
    pub turns: Vec<Turn>,
    /// Why it ended.
    pub stopped_by: StopReason,
}

impl Transcript {
    /// The conversation as plain text, one labelled block per turn, for the
    /// formatter to read.
    #[must_use]
    pub fn render(&self) -> String {
        self.turns
            .iter()
            .map(|turn| format!("[{}]\n{}", turn.agent, turn.text.trim()))
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}
