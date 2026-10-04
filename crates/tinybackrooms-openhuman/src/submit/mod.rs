//! The typed-output tool.
//!
//! The embed facade has no `response_format`, so structured output goes through
//! a host tool: the agent is told to call it, its parameters are the requested
//! JSON Schema, and the arguments it is called with are the answer. A [`Slots`]
//! registry keyed by session id carries those arguments back to the call that
//! asked, so one agent can serve many concurrent requests without sharing
//! state between them.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use serde_json::Value;
use tinytools::{Tool, ToolResult};

/// Where one request's tool arguments land.
pub(crate) type Slot = Arc<Mutex<Option<Value>>>;

/// The open slots, by session id.
#[derive(Debug, Default)]
pub(crate) struct Slots(Mutex<HashMap<String, Slot>>);

impl Slots {
    /// Open an empty slot for `session`.
    pub(crate) fn open(&self, session: &str) -> Slot {
        let slot = Slot::default();
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(session.to_owned(), Arc::clone(&slot));
        slot
    }

    /// The slot open for `session`, if any.
    pub(crate) fn get(&self, session: &str) -> Option<Slot> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(session)
            .cloned()
    }

    /// Forget `session`'s slot.
    pub(crate) fn close(&self, session: &str) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(session);
    }
}

/// What the tool is called and what it accepts.
#[derive(Debug, Clone)]
pub(crate) struct ToolDef {
    pub(crate) name: String,
    pub(crate) schema: Value,
}

/// The tool an agent calls to submit its answer.
pub(crate) struct SubmitTool {
    def: ToolDef,
    slot: Option<Slot>,
}

impl SubmitTool {
    /// A tool that records into `slot`, or refuses when there is none.
    pub(crate) fn new(def: ToolDef, slot: Option<Slot>) -> Self {
        Self { def, slot }
    }
}

#[async_trait]
impl Tool for SubmitTool {
    fn name(&self) -> &str {
        &self.def.name
    }

    fn description(&self) -> &'static str {
        "Submit your final answer. Call this exactly once, with the complete result."
    }

    fn parameters_schema(&self) -> Value {
        self.def.schema.clone()
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        let Some(slot) = &self.slot else {
            return Ok(ToolResult::error("no request is waiting for this answer"));
        };
        *slot.lock().unwrap_or_else(PoisonError::into_inner) = Some(args);
        Ok(ToolResult::success("recorded"))
    }
}

#[cfg(test)]
mod test;
