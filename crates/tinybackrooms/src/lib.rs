//! A configurable creativity engine.
//!
//! One call takes **input** (all the context), an **output spec** (a JSON
//! Schema every result must satisfy) and a **config** (how many variations, how
//! many turns, how to score), and returns ranked, typed items:
//! [`Engine::generate`].
//!
//! # How it works
//!
//! Each *variation* is a conversation between two agents in the style of
//! Andy Ayrey's Infinite Backrooms: no task to complete, each agent answering
//! the other, a relaxed command-line framing, and a high temperature. That is
//! where the ideas come from, and none of it is format-constrained. A second
//! pass then has a formatter agent read the transcript and call a typed tool
//! whose arguments are your schema; every item is validated and sent back for
//! repair when it fails. Finally the pooled items are scored (semantic
//! originality, elaboration, and an optional multi-rater judge that gates
//! novelty by usefulness), ranked, and summarized by set-level diversity
//! (`Vendi` score, distinct classes, fluency, flexibility, DAT).
//!
//! Splitting exploration from structure is deliberate. Constraining a model to
//! a format while it is still thinking costs it quality, so structure is
//! applied after the fact.
//!
//! # Entry points
//!
//! - [`GenerateRequest`] and [`GenerateResponse`]: the single request and its
//!   answer.
//! - [`Engine`]: runs a request against a [`Conversant`] (the model backend),
//!   optionally with a [`Memory`], an [`Embedder`] and an [`Observer`].
//! - [`ScriptedConversant`]: a deterministic backend for tests and offline use.
//!
//! # Example
//!
//! ```
//! use std::sync::Arc;
//! use serde_json::json;
//! use tinybackrooms::{Engine, GenerateRequest, ScriptedConversant};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let request: GenerateRequest = serde_json::from_value(json!({
//!     "input": {"brief": "name a cafe built inside a lighthouse"},
//!     "output": {
//!         "item_name": "cafe name",
//!         "schema": {
//!             "type": "object",
//!             "required": ["name"],
//!             "properties": {"name": {"type": "string"}}
//!         }
//!     },
//!     "config": {"variations": 2, "generations": 4, "items_per_variation": 2}
//! }))?;
//!
//! // A scripted backend stands in for real models.
//! let backend = ScriptedConversant::new(
//!     |turn| Ok(format!("{} opens a door", turn.seat.role)),
//!     |call| Ok(json!({"items": [
//!         {"name": format!("{} Lamp Room", call.seat.run_id)},
//!         {"name": format!("{} Salt Light", call.seat.run_id)},
//!     ]})),
//! );
//!
//! let engine = Engine::new(Arc::new(backend));
//! let response = tokio::runtime::Runtime::new()?.block_on(engine.generate(&request))?;
//! assert_eq!(response.items.len(), 4);
//! assert!(response.items[0].total >= response.items[3].total);
//! # Ok(())
//! # }
//! ```
//!
//! # What this crate does not hold
//!
//! No HTTP server (see `tinybackrooms-api`), no model client, and no async
//! runtime: models and memory arrive through the [`Conversant`] and [`Memory`]
//! ports, so the engine can be tested without a network and embedded in any
//! runtime. Per-turn timeouts belong to the backend for the same reason.

mod backrooms;
mod conversant;
mod embed;
mod engine;
mod error;
mod extract;
mod memory;
mod progress;
mod request;
mod response;
mod rng;
mod score;

pub use backrooms::{StopReason, Transcript, Turn};
pub use conversant::{
    BoxFuture, Conversant, Message, Role, ScriptedConversant, Seat, StructuredReply,
    StructuredRequest, TurnReply, TurnRequest, Usage,
};
pub use embed::{Embedder, HashingEmbedder, cosine};
pub use engine::Engine;
pub use error::{Error, Result};
pub use memory::{InMemoryMemory, Memory};
pub use progress::{Event, NoopObserver, Observer};
pub use request::{
    Budget, Constraints, Context, GenerateRequest, MAX_VARIATIONS, Mode, ModelSet, OutputSpec,
    RunConfig, ScorerKind, ScorerSpec, ScoringConfig, Seed, SeedKind, Template,
};
pub use response::{BodenClass, Diversity, GenerateResponse, ScoredItem};
