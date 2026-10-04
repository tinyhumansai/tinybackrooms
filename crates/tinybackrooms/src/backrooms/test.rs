//! Unit tests for the conversation loop and its prompts.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde_json::json;

use super::template::{LENSES, Side, opening_message, problem_block, system_prompt};
use super::*;
use crate::progress::NoopObserver;
use crate::{
    Constraints, Error, ModelSet, ScriptedConversant, Seed, SeedKind, Template, TurnRequest,
};

type Log = Arc<Mutex<Vec<TurnRequest>>>;

fn context() -> Context {
    Context {
        brief: "name a lighthouse cafe".into(),
        seeds: vec![
            Seed {
                kind: SeedKind::Text,
                text: "coastal town".into(),
            },
            Seed {
                kind: SeedKind::PriorIdea,
                text: "Beacon Brew".into(),
            },
        ],
        constraints: Constraints {
            must: vec!["be pronounceable".into()],
            must_not: vec!["use puns".into()],
        },
        memory_namespace: None,
    }
}

fn backend(reply: impl Fn(usize) -> String + Send + Sync + 'static) -> (ScriptedConversant, Log) {
    let log: Log = Arc::default();
    let sink = Arc::clone(&log);
    let backend = ScriptedConversant::new(
        move |request| {
            let mut log = sink.lock().unwrap();
            log.push(request.clone());
            Ok(reply(log.len()))
        },
        |_| Ok(json!(null)),
    );
    (backend, log)
}

async fn converse(
    config: &RunConfig,
    backend: &ScriptedConversant,
    deadline: Option<Instant>,
) -> Result<(Transcript, Usage)> {
    let context = context();
    let conversation = Conversation {
        run_id: "v0",
        index: 0,
        context: &context,
        recalled: &[],
        config,
        deadline,
    };
    run(&conversation, backend, &NoopObserver).await
}

#[tokio::test]
async fn alternates_agents_and_feeds_each_reply_to_the_other() {
    let (backend, log) = backend(|n| format!("reply {n}"));
    let config = RunConfig {
        generations: 4,
        ..RunConfig::default()
    };
    let (transcript, usage) = converse(&config, &backend, None).await.unwrap();

    let agents: Vec<_> = transcript.turns.iter().map(|t| t.agent.as_str()).collect();
    assert_eq!(agents, ["agent_a", "agent_b", "agent_a", "agent_b"]);
    assert_eq!(transcript.stopped_by, StopReason::Generations);
    assert_eq!(usage.calls, 4);

    let log = log.lock().unwrap();
    // B's first prompt is A's first reply, as a user message.
    assert_eq!(log[1].messages.len(), 1);
    assert_eq!(log[1].messages[0], Message::user("reply 1"));
    // A's second turn sees: opening, its own reply, then B's reply.
    assert_eq!(log[2].messages.len(), 3);
    assert_eq!(log[2].messages[1], Message::assistant("reply 1"));
    assert_eq!(log[2].messages[2], Message::user("reply 2"));
}

#[tokio::test]
async fn stops_early_on_the_interrupt_sequence() {
    let (backend, _) = backend(|n| {
        if n == 2 {
            "done ^C^C".into()
        } else {
            "more".into()
        }
    });
    let config = RunConfig {
        generations: 10,
        ..RunConfig::default()
    };
    let (transcript, _) = converse(&config, &backend, None).await.unwrap();
    assert_eq!(transcript.turns.len(), 2);
    assert_eq!(transcript.stopped_by, StopReason::Interrupt);
}

#[tokio::test]
async fn max_turns_caps_generations() {
    let (backend, _) = backend(|_| "x".into());
    let mut config = RunConfig {
        generations: 10,
        ..RunConfig::default()
    };
    config.budget.max_turns = Some(3);
    let (transcript, _) = converse(&config, &backend, None).await.unwrap();
    assert_eq!(transcript.turns.len(), 3);
}

#[tokio::test]
async fn a_passed_deadline_stops_before_the_first_turn() {
    let (backend, log) = backend(|_| "x".into());
    let (transcript, _) = converse(&RunConfig::default(), &backend, Some(Instant::now()))
        .await
        .unwrap();
    assert_eq!(transcript.turns.len(), 0);
    assert_eq!(transcript.stopped_by, StopReason::WallClock);
    assert_eq!(log.lock().unwrap().len(), 0);
}

#[tokio::test]
async fn brainstorm_keeps_one_agent_talking_to_itself() {
    let (backend, log) = backend(|n| format!("thought {n}"));
    let config = RunConfig {
        mode: Mode::Brainstorm,
        generations: 3,
        ..RunConfig::default()
    };
    let (transcript, _) = converse(&config, &backend, None).await.unwrap();
    assert!(transcript.turns.iter().all(|t| t.agent == "agent_a"));
    let log = log.lock().unwrap();
    let last = &log[2].messages;
    assert_eq!(last.len(), 5);
    assert_eq!(last[4], Message::user(CONTINUE));
}

#[tokio::test]
async fn routes_models_per_agent() {
    let (backend, log) = backend(|_| "x".into());
    let config = RunConfig {
        generations: 2,
        models: ModelSet {
            agent_a: Some("model-a".into()),
            agent_b: Some("model-b".into()),
            ..ModelSet::default()
        },
        ..RunConfig::default()
    };
    converse(&config, &backend, None).await.unwrap();
    let log = log.lock().unwrap();
    assert_eq!(log[0].model.as_deref(), Some("model-a"));
    assert_eq!(log[1].model.as_deref(), Some("model-b"));
}

#[tokio::test]
async fn gives_the_two_agents_different_lenses_reproducibly() {
    let (backend, log) = backend(|_| "x".into());
    let config = RunConfig {
        generations: 2,
        seed: 11,
        ..RunConfig::default()
    };
    converse(&config, &backend, None).await.unwrap();
    let first = log.lock().unwrap().clone();
    let lens_of = |request: &TurnRequest| {
        LENSES
            .iter()
            .position(|lens| request.system.contains(lens))
            .unwrap()
    };
    assert_ne!(lens_of(&first[0]), lens_of(&first[1]));

    let (again, second) = self::backend(|_| "x".into());
    converse(&config, &again, None).await.unwrap();
    let second = second.lock().unwrap();
    assert_eq!(first[0].system, second[0].system);
}

#[tokio::test]
async fn propagates_a_failed_turn() {
    let failing = ScriptedConversant::new(
        |_| Err(Error::Conversant("down".into())),
        |_| Ok(json!(null)),
    );
    let result = converse(&RunConfig::default(), &failing, None).await;
    assert_eq!(result.unwrap_err(), Error::Conversant("down".into()));
}

#[test]
fn problem_block_lists_every_populated_section() {
    let block = problem_block(&context(), &["Old Idea".to_owned()]);
    for expected in [
        "name a lighthouse cafe",
        "BACKGROUND",
        "- coastal town",
        "IDEAS TO MOVE AWAY FROM",
        "- Beacon Brew",
        "DO NOT REPEAT",
        "- Old Idea",
        "EVERY IDEA MUST",
        "NO IDEA MAY",
    ] {
        assert!(block.contains(expected), "missing {expected:?} in\n{block}");
    }
}

#[test]
fn problem_block_omits_empty_sections() {
    let bare = Context {
        brief: "b".into(),
        seeds: vec![],
        constraints: Constraints::default(),
        memory_namespace: None,
    };
    assert_eq!(problem_block(&bare, &[]), "PROBLEM\nb\n");
}

#[test]
fn every_mode_and_template_has_distinct_prompts() {
    for template in [Template::Cli, Template::Fugue, Template::Student] {
        let a = system_prompt(Mode::Backrooms, template, Side::A, "lens", "p");
        let b = system_prompt(Mode::Backrooms, template, Side::B, "lens", "p");
        assert_ne!(a, b);
        assert!(a.contains("^C^C") && a.contains("lens"));
        assert_ne!(opening_message(Mode::Backrooms, template).len(), 0);
    }
    let proposer = system_prompt(Mode::CriticPair, Template::Cli, Side::A, "l", "p");
    let critic = system_prompt(Mode::CriticPair, Template::Cli, Side::B, "l", "p");
    assert_ne!(proposer, critic);
    assert!(system_prompt(Mode::Brainstorm, Template::Cli, Side::A, "l", "p").contains("alone"));
    assert_ne!(opening_message(Mode::Brainstorm, Template::Cli).len(), 0);
    assert_ne!(opening_message(Mode::CriticPair, Template::Cli).len(), 0);
}

#[test]
fn renders_a_transcript_as_labelled_blocks() {
    let transcript = Transcript {
        run_id: "v0".into(),
        turns: vec![
            Turn {
                agent: "agent_a".into(),
                text: " ls \n".into(),
            },
            Turn {
                agent: "agent_b".into(),
                text: "ideas/".into(),
            },
        ],
        stopped_by: StopReason::Generations,
    };
    assert_eq!(transcript.render(), "[agent_a]\nls\n\n[agent_b]\nideas/");
}
