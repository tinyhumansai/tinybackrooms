//! The Infinite-Backrooms conversation loop.
//!
//! One *variation* is one conversation. The engine owns it: it keeps a separate
//! history for each agent, asks whichever agent is next for a free-text turn
//! through the [`Conversant`] port, and hands that reply to the other agent as a
//! user message. Nothing constrains the format of a turn; structure is applied
//! later, in a separate pass, because constraining a model while it is still
//! exploring costs it quality.
//!
//! A conversation ends when it reaches its turn count, when an agent types the
//! `^C^C` interrupt, or when the wall-clock deadline passes. The turn count is
//! the smaller of `generations` and the budget's `max_turns`.
//!
//! Each variation draws two different *lenses* from a seeded generator so the
//! same model talking to itself starts from different angles; see `template`.

mod template;
mod types;

use std::time::Instant;

use crate::progress::{Event, Observer};
use crate::rng::Rng;
use crate::{Context, Conversant, Message, Mode, Result, RunConfig, Seat, TurnRequest, Usage};
use template::{CONTINUE, INTERRUPT, LENSES, Side, opening_message, problem_block, system_prompt};
pub use types::{StopReason, Transcript, Turn};

/// Everything one conversation needs.
pub(crate) struct Conversation<'a> {
    /// The variation id, such as `v0`.
    pub(crate) run_id: &'a str,
    /// The variation's position, which seeds its lens draw.
    pub(crate) index: u64,
    /// The problem the agents discuss.
    pub(crate) context: &'a Context,
    /// Items remembered from earlier runs, to be avoided.
    pub(crate) recalled: &'a [String],
    /// The run's configuration.
    pub(crate) config: &'a RunConfig,
    /// Stop starting turns after this instant.
    pub(crate) deadline: Option<Instant>,
}

/// Run one conversation to its end.
///
/// # Errors
///
/// Returns whatever the [`Conversant`] returns when a turn fails; the partial
/// conversation is discarded.
pub(crate) async fn run(
    conversation: &Conversation<'_>,
    backend: &dyn Conversant,
    observer: &dyn Observer,
) -> Result<(Transcript, Usage)> {
    let config = conversation.config;
    let mut rng = Rng::new(config.seed, conversation.index);
    let lens_a = rng.below(LENSES.len());
    let lens_b = (lens_a + 1 + rng.below(LENSES.len() - 1)) % LENSES.len();
    let problem = problem_block(conversation.context, conversation.recalled);
    let system_a = system_prompt(
        config.mode,
        config.template,
        Side::A,
        LENSES[lens_a],
        &problem,
    );
    let system_b = system_prompt(
        config.mode,
        config.template,
        Side::B,
        LENSES[lens_b],
        &problem,
    );

    let solo = config.mode == Mode::Brainstorm;
    let turns_allowed = config
        .budget
        .max_turns
        .map_or(config.generations, |cap| cap.min(config.generations));
    let mut history_a = vec![Message::user(opening_message(config.mode, config.template))];
    let mut history_b: Vec<Message> = Vec::new();
    let mut turns = Vec::new();
    let mut usage = Usage::default();
    let mut stopped_by = StopReason::Generations;

    for index in 0..turns_allowed {
        if conversation
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            stopped_by = StopReason::WallClock;
            break;
        }
        let a_speaks = solo || index % 2 == 0;
        let (role, model, system, history) = if a_speaks {
            ("agent_a", &config.models.agent_a, &system_a, &history_a)
        } else {
            ("agent_b", &config.models.agent_b, &system_b, &history_b)
        };
        let reply = backend
            .turn(TurnRequest {
                seat: Seat {
                    run_id: conversation.run_id.to_owned(),
                    role: role.to_owned(),
                },
                model: model.clone(),
                temperature: config.temperature,
                system: system.clone(),
                messages: history.clone(),
            })
            .await?;
        usage.add(reply.usage);
        let text = reply.text;
        observer.event(Event::Turn {
            run_id: conversation.run_id.to_owned(),
            index,
            agent: role.to_owned(),
            text: text.clone(),
        });
        let interrupted = text.contains(INTERRUPT);
        if a_speaks {
            history_a.push(Message::assistant(text.clone()));
            if solo {
                history_a.push(Message::user(CONTINUE));
            } else {
                history_b.push(Message::user(text.clone()));
            }
        } else {
            history_b.push(Message::assistant(text.clone()));
            history_a.push(Message::user(text.clone()));
        }
        turns.push(Turn {
            agent: role.to_owned(),
            text,
        });
        if interrupted {
            stopped_by = StopReason::Interrupt;
            break;
        }
    }

    let transcript = Transcript {
        run_id: conversation.run_id.to_owned(),
        turns,
        stopped_by,
    };
    Ok((transcript, usage))
}

#[cfg(test)]
mod test;
