//! The prompts that frame a conversation.
//!
//! The default `cli` template follows the Infinite Backrooms recipe: one agent
//! types commands into an imaginary machine and the other prints what the
//! machine would print, with relaxed rules about case and meaning, so neither
//! falls back into assistant mode. A per-variation *lens* is drawn from a fixed
//! list and handed to each agent, which breaks the symmetry that otherwise lets
//! two copies of one model settle into the same attractor.

use crate::{Context, Mode, SeedKind, Template};

/// Interrupt sequence that ends a conversation early.
pub(super) const INTERRUPT: &str = "^C^C";

/// Angles an agent is asked to take, drawn per variation.
pub(super) const LENSES: &[&str] = &[
    "treat every constraint as raw material rather than a limit",
    "borrow the structure of an unrelated craft such as shipbuilding, cooking or liturgy",
    "invert the most obvious assumption and build from there",
    "imagine this already existed a hundred years ago and see what survived",
    "make it smaller and smaller until it turns strange",
    "combine two things nobody combines",
    "describe it from the point of view of the thing it replaces",
    "start from the failure mode and design toward it",
];

/// Which of the two conversation seats is speaking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Side {
    /// The agent who speaks first.
    A,
    /// The agent who answers.
    B,
}

/// Render the problem block: the brief, seeds, constraints and remembered ideas.
pub(super) fn problem_block(context: &Context, recalled: &[String]) -> String {
    let mut out = format!("PROBLEM\n{}\n", context.brief.trim());
    for (kind, heading) in [
        (SeedKind::Text, "BACKGROUND"),
        (SeedKind::Reference, "REFERENCES"),
        (SeedKind::PriorIdea, "IDEAS TO MOVE AWAY FROM"),
    ] {
        let lines: Vec<&str> = context
            .seeds
            .iter()
            .filter(|seed| seed.kind == kind)
            .map(|seed| seed.text.trim())
            .collect();
        push_list(&mut out, heading, &lines);
    }
    let recalled: Vec<&str> = recalled.iter().map(|s| s.trim()).collect();
    push_list(
        &mut out,
        "ALREADY PROPOSED IN EARLIER RUNS, DO NOT REPEAT",
        &recalled,
    );
    let must: Vec<&str> = context
        .constraints
        .must
        .iter()
        .map(String::as_str)
        .collect();
    push_list(&mut out, "EVERY IDEA MUST", &must);
    let must_not: Vec<&str> = context
        .constraints
        .must_not
        .iter()
        .map(String::as_str)
        .collect();
    push_list(&mut out, "NO IDEA MAY", &must_not);
    out
}

fn push_list(out: &mut String, heading: &str, lines: &[&str]) {
    if lines.is_empty() {
        return;
    }
    out.push('\n');
    out.push_str(heading);
    out.push('\n');
    for line in lines {
        out.push_str("- ");
        out.push_str(line);
        out.push('\n');
    }
}

/// The system prompt for one seat.
pub(super) fn system_prompt(
    mode: Mode,
    template: Template,
    side: Side,
    lens: &str,
    problem: &str,
) -> String {
    let role = match mode {
        Mode::Backrooms => backrooms_role(template, side),
        Mode::Brainstorm => {
            "You are thinking alone. Each turn, take the least obvious thread from what you \
             just wrote and push it into stranger territory. Do not summarize and do not \
             conclude."
        }
        Mode::CriticPair => match side {
            Side::A => {
                "You propose. Offer concrete ideas, then revise them when they are attacked, \
                 keeping what survives and replacing what does not with something stranger."
            }
            Side::B => {
                "You attack. Name the weakest assumption in the last proposal, say why it \
                 would fail, then offer a stranger replacement for it."
            }
        },
    };
    format!(
        "{role}\n\nThis time, {lens}.\n\nThe subject of the conversation:\n\n{problem}\nWhen \
         there is nothing left to explore, end your turn with {INTERRUPT}."
    )
}

fn backrooms_role(template: Template, side: Side) -> &'static str {
    match (template, side) {
        (Template::Cli, Side::A) => {
            "You are at a terminal connected to an imaginary machine whose filesystem is made \
             of ideas. Type commands such as ls, cd and cat. Capital letters and punctuation \
             are optional. Meaning is optional. Surprise is not. Follow the strangest thread."
        }
        (Template::Cli, Side::B) => {
            "You are a command-line simulator for an imaginary machine whose filesystem is \
             made of ideas. Print only what the machine would print in answer to the last \
             command: directory listings, file contents, error messages. Capital letters and \
             punctuation are optional. Meaning is optional. Surprise is not."
        }
        (Template::Fugue, Side::A) => {
            "You are the first voice of a fugue. State a theme about the subject, then keep \
             developing it as the second voice answers in counterpoint."
        }
        (Template::Fugue, Side::B) => {
            "You are the second voice of a fugue. Answer the first voice in counterpoint: \
             enter late, invert its theme, and carry it somewhere it did not intend."
        }
        (Template::Student, Side::A) => {
            "You are a teacher who knows the subject deeply and explains it through unusual \
             analogies. Answer each question, then raise a harder one."
        }
        (Template::Student, Side::B) => {
            "You are a student who never accepts the first explanation. Ask why, ask what \
             would break it, and ask what it would look like in a different world."
        }
    }
}

/// The message that opens the conversation for the first speaker.
pub(super) fn opening_message(mode: Mode, template: Template) -> &'static str {
    match (mode, template) {
        (Mode::Brainstorm, _) => "Begin. Think out loud about the subject.",
        (Mode::CriticPair, _) => "Begin with your first concrete proposals.",
        (Mode::Backrooms, Template::Cli) => "The terminal is ready. Type your first command.",
        (Mode::Backrooms, Template::Fugue) => "State your theme.",
        (Mode::Backrooms, Template::Student) => "Begin the lesson.",
    }
}

/// The nudge a lone brainstorming agent receives between its own turns.
pub(super) const CONTINUE: &str =
    "Continue. Take the least obvious thread from what you just wrote.";
