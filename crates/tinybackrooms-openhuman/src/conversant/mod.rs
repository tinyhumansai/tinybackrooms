//! [`OpenHumanConversant`]: the engine's `Conversant` port on `OpenHuman` agents.
//!
//! Every seat the engine asks for becomes an `OpenHuman` agent, so a creative
//! conversation really is `OpenHuman` agents answering each other. The engine
//! owns the loop and the histories; this crate only answers a turn.
//!
//! - **`turn`** runs the agent on a fresh session seeded with the history the
//!   engine supplies and sends the final message. Free text in, free text out.
//! - **`structured`** gives the agent one host tool whose parameters are the
//!   requested JSON Schema and returns the arguments it called it with.
//!
//! A provider that answers with nothing is asked again, up to
//! `OpenHumanConfig::empty_retries` times, on a fresh session.
//!
//! Timeouts are enforced here, per turn, because the engine links no timer. A
//! timed-out turn is dropped, which is the only cancellation the embed facade
//! offers.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use openhuman_embed::{Agent, AgentSpec, Provider, Runtime, Workspace};
use tinybackrooms::{
    BoxFuture, Conversant, Error, Message, Result, Role, StructuredReply, StructuredRequest,
    TurnReply, TurnRequest, Usage,
};
use tokio::task::JoinHandle;

use crate::agents::{agent_id, chat_spec, structured_spec, temperature_key};
use crate::config::{OpenHumanConfig, runtime_config};
use crate::stub;
use crate::submit::{Slots, ToolDef};

/// Runs engine seats as `OpenHuman` agents.
///
/// At most one can exist per process: `OpenHuman` allows a single `Runtime`.
pub struct OpenHumanConversant {
    runtime: Runtime,
    agents: Mutex<HashMap<String, Agent>>,
    slots: Arc<Slots>,
    sessions: AtomicU64,
    default_model: String,
    timeout: Duration,
    empty_retries: u32,
    backend: JoinHandle<()>,
}

impl std::fmt::Debug for OpenHumanConversant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenHumanConversant")
            .finish_non_exhaustive()
    }
}

impl Drop for OpenHumanConversant {
    fn drop(&mut self) {
        self.backend.abort();
    }
}

/// Whether `error` is the provider answering with nothing, which is worth
/// asking again rather than failing the whole variation.
fn is_empty_response(error: &Error) -> bool {
    matches!(error, Error::Conversant(message) if message.contains("empty response"))
}

fn failure(context: &str, error: impl std::fmt::Display) -> Error {
    Error::Conversant(format!("{context}: {error}"))
}

impl OpenHumanConversant {
    /// Boot an `OpenHuman` runtime that talks to `config`'s provider.
    ///
    /// Must be called from a tokio runtime whose worker threads have a large
    /// stack (`OpenHuman` overflows the 2 MiB default; 16 MiB works).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Conversant`] when the local backend stub cannot start,
    /// or when a runtime already exists in this process or cannot be built.
    pub async fn new(config: OpenHumanConfig) -> Result<Self> {
        let (backend_url, backend) = stub::spawn()
            .await
            .map_err(|e| failure("cannot start the backend stub", e))?;
        let provider = Provider::openai_compatible(config.base_url, config.api_key)
            .model(config.model.clone());
        let runtime = Box::pin(
            Runtime::builder()
                .config(runtime_config())
                .workspace(Workspace::Ephemeral)
                .backend_url(backend_url)
                .provider(provider)
                .access(openhuman_embed::Access::readonly())
                .build(),
        )
        .await
        .map_err(|e| {
            backend.abort();
            failure("cannot start the OpenHuman runtime", e)
        })?;
        Ok(Self {
            runtime,
            agents: Mutex::default(),
            slots: Arc::default(),
            sessions: AtomicU64::new(0),
            default_model: config.model,
            timeout: config.turn_timeout,
            empty_retries: config.empty_retries,
            backend,
        })
    }

    fn model<'a>(&'a self, requested: Option<&'a str>) -> &'a str {
        requested.unwrap_or(&self.default_model)
    }

    fn next_session(&self) -> String {
        format!("tbr-{}", self.sessions.fetch_add(1, Ordering::Relaxed))
    }

    /// The agent for `id`, building it from `spec` the first time it is asked for.
    fn agent(&self, id: &str, spec: impl FnOnce() -> AgentSpec) -> Result<Agent> {
        let mut agents = self.agents.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(agent) = agents.get(id) {
            return Ok(agent.clone());
        }
        let agent = self
            .runtime
            .agent(spec())
            .map_err(|e| failure(&format!("cannot create agent {id}"), e))?;
        agents.insert(id.to_owned(), agent.clone());
        Ok(agent)
    }

    /// Run `messages` on a fresh session of `agent` and return the final text.
    async fn run(
        &self,
        agent: &Agent,
        session: &str,
        system: &str,
        messages: &[Message],
        model: &str,
    ) -> Result<(String, Usage)> {
        let (last, history) = messages
            .split_last()
            .ok_or_else(|| Error::Conversant("a turn needs at least one message".to_owned()))?;
        let rows: Vec<(String, String)> = std::iter::once(("system".to_owned(), system.to_owned()))
            .chain(
                history
                    .iter()
                    .map(|message| (role_name(message.role).to_owned(), message.content.clone())),
            )
            .collect();
        let mut turn = agent
            .turn(last.content.clone())
            .session(session)
            .model(model);
        turn = turn.seed(rows);
        let outcome = Box::pin(tokio::time::timeout(self.timeout, turn.send()))
            .await
            .map_err(|_| failure("turn timed out", format!("{}s", self.timeout.as_secs())))?
            .map_err(|e| failure("turn failed", e))?;
        let usage = outcome.usage.map_or_else(Usage::default, |spent| Usage {
            input_tokens: spent.input_tokens,
            output_tokens: spent.output_tokens,
            calls: 0,
        });
        Ok((outcome.reply, Usage { calls: 1, ..usage }))
    }
}

fn role_name(role: Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}

impl Conversant for OpenHumanConversant {
    fn turn(&self, request: TurnRequest) -> BoxFuture<'_, Result<TurnReply>> {
        Box::pin(async move {
            let model = self.model(request.model.as_deref());
            let temperature = temperature_key(request.temperature);
            let id = agent_id(&request.seat.role, &[&request.system, model, &temperature]);
            let agent = self.agent(&id, || chat_spec(&id, &request.system, request.temperature))?;
            let mut attempt = 0;
            let (text, usage) = loop {
                let session = self.next_session();
                let outcome = Box::pin(self.run(
                    &agent,
                    &session,
                    &request.system,
                    &request.messages,
                    model,
                ))
                .await;
                match outcome {
                    Err(error) if attempt < self.empty_retries && is_empty_response(&error) => {
                        attempt += 1;
                    }
                    other => break other?,
                }
            };
            Ok(TurnReply { text, usage })
        })
    }

    fn structured(&self, request: StructuredRequest) -> BoxFuture<'_, Result<StructuredReply>> {
        Box::pin(async move {
            let model = self.model(request.model.as_deref());
            let def = ToolDef {
                name: request.tool.clone(),
                schema: request.schema.clone(),
            };
            let schema = request.schema.to_string();
            let id = agent_id(
                &request.seat.role,
                &[&request.system, model, &def.name, &schema],
            );
            let agent = self.agent(&id, || {
                structured_spec(&id, &request.system, &def, Arc::clone(&self.slots))
            })?;
            let mut attempt = 0;
            let (usage, slot) = loop {
                let session = self.next_session();
                let slot = self.slots.open(&session);
                let outcome = Box::pin(self.run(
                    &agent,
                    &session,
                    &request.system,
                    &request.messages,
                    model,
                ))
                .await;
                self.slots.close(&session);
                match outcome {
                    Err(error) if attempt < self.empty_retries && is_empty_response(&error) => {
                        attempt += 1;
                    }
                    other => break (other?.1, slot),
                }
            };
            let value = slot
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take()
                .ok_or_else(|| {
                    Error::Conversant(format!("the agent never called `{}`", request.tool))
                })?;
            Ok(StructuredReply { value, usage })
        })
    }
}

#[cfg(test)]
mod test;
