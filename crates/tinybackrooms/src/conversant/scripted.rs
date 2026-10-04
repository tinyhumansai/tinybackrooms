//! A deterministic [`Conversant`] for tests, examples and offline servers.

use std::fmt;
use std::sync::Arc;

use super::{
    BoxFuture, Conversant, StructuredReply, StructuredRequest, TurnReply, TurnRequest, Usage,
};
use crate::Result;

type TurnFn = dyn Fn(&TurnRequest) -> Result<String> + Send + Sync;
type StructuredFn = dyn Fn(&StructuredRequest) -> Result<serde_json::Value> + Send + Sync;

/// A [`Conversant`] whose answers come from two closures.
///
/// Nothing here calls a model. Use it to test the engine, to run the bundled
/// offline examples, and to serve the API without credentials.
#[derive(Clone)]
pub struct ScriptedConversant {
    turn: Arc<TurnFn>,
    structured: Arc<StructuredFn>,
}

impl fmt::Debug for ScriptedConversant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScriptedConversant").finish_non_exhaustive()
    }
}

impl ScriptedConversant {
    /// Answer turns with `turn` and typed requests with `structured`.
    pub fn new(
        turn: impl Fn(&TurnRequest) -> Result<String> + Send + Sync + 'static,
        structured: impl Fn(&StructuredRequest) -> Result<serde_json::Value> + Send + Sync + 'static,
    ) -> Self {
        Self {
            turn: Arc::new(turn),
            structured: Arc::new(structured),
        }
    }
}

impl Conversant for ScriptedConversant {
    fn turn(&self, request: TurnRequest) -> BoxFuture<'_, Result<TurnReply>> {
        let reply = (self.turn)(&request).map(|text| TurnReply {
            usage: Usage {
                calls: 1,
                ..Usage::default()
            },
            text,
        });
        Box::pin(async move { reply })
    }

    fn structured(&self, request: StructuredRequest) -> BoxFuture<'_, Result<StructuredReply>> {
        let reply = (self.structured)(&request).map(|value| StructuredReply {
            usage: Usage {
                calls: 1,
                ..Usage::default()
            },
            value,
        });
        Box::pin(async move { reply })
    }
}
