//! The `OpenHuman` backend for `tinybackrooms`.
//!
//! [`OpenHumanConversant`] implements the engine's `Conversant` port on
//! `OpenHuman` agents, so a creative conversation really is agents answering
//! each other, and typed output arrives through a host tool whose parameters are
//! the caller's JSON Schema.
//!
//! ```no_run
//! # async fn demo() -> tinybackrooms::Result<()> {
//! use std::sync::Arc;
//! use tinybackrooms::Engine;
//! use tinybackrooms_openhuman::{OpenHumanConfig, OpenHumanConversant};
//!
//! let config = OpenHumanConfig::new("https://openrouter.ai/api/v1", "sk-...", "openai/gpt-oss-120b");
//! let engine = Engine::new(Arc::new(OpenHumanConversant::new(config).await?));
//! # let _ = engine;
//! # Ok(())
//! # }
//! ```
//!
//! # Process constraints
//!
//! - **One runtime per process.** `OpenHuman` allows a single `Runtime`; a second
//!   [`OpenHumanConversant::new`] fails. Agents are the unit of multiplicity.
//! - **A large worker stack.** Build the tokio runtime with
//!   `thread_stack_size(16 * 1024 * 1024)`; the 2 MiB default overflows.
//! - **Timeouts live here.** The engine links no timer, so each turn is cut off
//!   after [`OpenHumanConfig::turn_timeout`]. Dropping a turn is the only
//!   cancellation the embed facade offers.
//!
//! # Behaviors worth knowing
//!
//! - Each turn seeds an explicit `system` row. Without it `OpenHuman` wraps its own
//!   assistant persona around the prompt, and a seeded session drops the prompt.
//! - Temperature lives on the agent definition, because a per-turn override is
//!   ignored; a different temperature is a different agent.
//! - A provider 5xx is retried with backoff, so it may surface as a timeout.
//!
//! See `docs/plans/openhuman-adapter.md` for the evidence.
//!
//! # What this crate does not do
//!
//! It provides no `Memory` backend. The embed facade has no typed host memory
//! API, so recall and storage stay on the engine's `Memory` port (in-process by
//! default); see `docs/plans/openhuman-adapter.md`.

mod agents;
mod config;
mod conversant;
mod stub;
mod submit;

pub use config::{DEFAULT_TURN_TIMEOUT, OpenHumanConfig};
pub use conversant::OpenHumanConversant;
