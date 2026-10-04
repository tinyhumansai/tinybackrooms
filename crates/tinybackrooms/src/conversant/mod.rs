//! The model-backend port.
//!
//! A [`Conversant`] answers one turn for one agent. The engine owns the
//! conversation: it keeps each agent's history, decides who speaks next, and
//! passes one agent's reply to the other as a user message. A backend only
//! answers; it holds no loop. That keeps the engine testable against
//! [`ScriptedConversant`] and lets the `OpenHuman` adapter (or any other
//! provider) plug in behind the same two methods.
//!
//! The two methods mirror the two phases of a run: [`Conversant::turn`] is
//! unconstrained free text for the exploration phase, and
//! [`Conversant::structured`] is a typed tool call for the extraction phase.
//! Format constraints are kept out of exploration on purpose: constraining a
//! model while it is still thinking costs it quality, so structure is applied
//! in a separate second pass.

mod scripted;
mod types;

use std::future::Future;
use std::pin::Pin;

pub use scripted::ScriptedConversant;
pub use types::{
    Message, Role, Seat, StructuredReply, StructuredRequest, TurnReply, TurnRequest, Usage,
};

use crate::Result;

/// A boxed, sendable future returned by the port, so the trait stays object safe.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A model backend that answers turns.
///
/// Implementations enforce their own per-turn timeout; the engine does not
/// link a timer.
pub trait Conversant: Send + Sync {
    /// Answer one free-text turn.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Conversant`](crate::Error::Conversant) when the backend
    /// cannot answer.
    fn turn(&self, request: TurnRequest) -> BoxFuture<'_, Result<TurnReply>>;

    /// Answer by calling the typed tool described in the request.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Conversant`](crate::Error::Conversant) when the backend
    /// cannot answer or the agent never calls the tool.
    fn structured(&self, request: StructuredRequest) -> BoxFuture<'_, Result<StructuredReply>>;
}

#[cfg(test)]
mod test;
