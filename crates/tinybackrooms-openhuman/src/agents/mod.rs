//! How a seat becomes an `OpenHuman` agent.
//!
//! An agent is identified by what it *is*: its role, its system prompt, and for
//! a typed call its tool. Those hash into the agent id, so every variation and
//! every request that asks for the same thing shares one agent handle, and a
//! different prompt gets a different agent. Conversation state is not kept on
//! the agent: each turn runs on a fresh session seeded with the history the
//! engine supplies, so a handle can serve many turns at once.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use openhuman_embed::{Access, AgentDefinitionSpec, AgentSpec, HostTurnTools, ToolScopeSpec};
use tinytools::Tool;

use crate::submit::{Slots, SubmitTool, ToolDef};

/// Most agent turns allowed for a typed call: call the tool, then close out.
const STRUCTURED_ITERATIONS: usize = 4;

/// How a temperature enters an agent id, so near-equal values share an agent.
pub(crate) fn temperature_key(temperature: f32) -> String {
    format!("{temperature:.2}")
}

/// A stable agent id for `role` and whatever distinguishes it, valid under
/// `OpenHuman`'s `^[a-z0-9][a-z0-9_-]{0,63}$`.
pub(crate) fn agent_id(role: &str, distinguishing: &[&str]) -> String {
    let mut hasher = DefaultHasher::new();
    distinguishing.hash(&mut hasher);
    let role: String = role
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .take(40)
        .collect();
    format!("tbr-{role}-{:08x}", hasher.finish() & 0xffff_ffff)
}

/// A conversational agent with no tools: it only talks.
///
/// The temperature is part of the definition, because a per-turn override is
/// not applied by the harness: every turn would otherwise sample at its default
/// of 0.4, which defeats a loop that depends on high-temperature exploration.
pub(crate) fn chat_spec(id: &str, system: &str, temperature: f32) -> AgentSpec {
    AgentSpec::new(id)
        .definition(
            AgentDefinitionSpec::new()
                .system_prompt(system)
                .tools(ToolScopeSpec::Named(Vec::new()))
                .max_iterations(1)
                .temperature(temperature_key(temperature).parse().unwrap_or(1.0)),
        )
        .access(Access::readonly())
}

/// An agent whose only tool is the typed-output tool described by `def`.
pub(crate) fn structured_spec(
    id: &str,
    system: &str,
    def: &ToolDef,
    slots: Arc<Slots>,
) -> AgentSpec {
    let belt = def.clone();
    AgentSpec::new(id)
        .definition(
            AgentDefinitionSpec::new()
                .system_prompt(system)
                .tools(ToolScopeSpec::Named(vec![def.name.clone()]))
                .max_iterations(STRUCTURED_ITERATIONS),
        )
        .access(Access::readonly())
        .tools(move |turn| {
            let slot = turn.session_id().and_then(|session| slots.get(session));
            HostTurnTools::advertised(vec![
                Box::new(SubmitTool::new(belt.clone(), slot)) as Box<dyn Tool>
            ])
        })
}

#[cfg(test)]
mod test;
