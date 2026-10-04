//! Progress events emitted while a run executes.
//!
//! The engine reports what it is doing through an [`Observer`] so a caller can
//! stream it (the HTTP server turns these into server-sent events) without the
//! engine knowing about transports or runtimes. Events describe progress only;
//! the final answer is the returned response.

use serde::{Deserialize, Serialize};

/// One thing that happened during a run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// A variation began its conversation.
    VariationStarted {
        /// The variation, such as `v0`.
        run_id: String,
    },
    /// An agent finished a conversation turn.
    Turn {
        /// The variation this turn belongs to.
        run_id: String,
        /// Zero-based position within the variation's conversation.
        index: u32,
        /// Who spoke, such as `agent_a`.
        agent: String,
        /// What they said.
        text: String,
    },
    /// A variation's transcript was turned into typed items.
    Extracted {
        /// The variation.
        run_id: String,
        /// Items that satisfied the schema.
        valid: u32,
        /// Items dropped for failing it after repair attempts.
        invalid: u32,
    },
    /// A variation failed and was dropped from the run.
    VariationFailed {
        /// The variation.
        run_id: String,
        /// Why, as a message.
        reason: String,
    },
    /// Scoring and selection finished.
    Scored {
        /// Items kept after selection.
        items: u32,
    },
}

/// Receives progress events. Implementations must not block.
pub trait Observer: Send + Sync {
    /// Handle one event.
    fn event(&self, event: Event);
}

/// An [`Observer`] that discards everything.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopObserver;

impl Observer for NoopObserver {
    fn event(&self, _event: Event) {}
}

#[cfg(test)]
mod test;
