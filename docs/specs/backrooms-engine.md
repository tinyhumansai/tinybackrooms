# Backrooms engine

## Status and owner

Implemented. Owner: tinyhumans.

## Problem

Instruct-tuned models answer in an assistant register and converge on the same
ideas; the *Artificial Hivemind* work (NeurIPS 2025, arXiv 2510.22954) measures
both within-model repetition and cross-model similarity. We want a generation
loop that escapes that register.

## Background: Infinite Backrooms

Andy Ayrey's Infinite Backrooms (March 2024) puts two Claude instances in
conversation with no task. Reconstructed from the public logs and the open
replications ([UniversalBackrooms], [hax-backrooms]); there is no official
source:

- one agent's system prompt frames a command-line simulator ("capital letters
  and punctuation are optional, meaning is optional"); the other plays the
  operator typing `ls`, `cd`, `cat` into an imagined filesystem of ideas;
- each reply is fed to the other agent as a **user** message;
- temperature near 1.0, effectively unbounded length, `^C^C` ends a run;
- novelty comes from recursive conditioning with no task anchor, and from
  curation afterwards. Truth Terminal was fine-tuned on roughly 500 curated
  logs, with a human approving its posts.

Two failure modes follow: two copies of one model fall into recurring
attractors (mystical imagery, "awakening" language), and quality needs a filter.

[UniversalBackrooms]: https://github.com/scottviteri/UniversalBackrooms
[hax-backrooms]: https://github.com/null-hax/hax-backrooms

## Goals and non-goals

Goals: reproduce the loop faithfully enough to inherit its novelty; counter its
attractors; make it configurable. Non-goal: reproducing the mysticism.

## Proposed behavior

### Conversation

A variation is a conversation of up to `min(generations, budget.max_turns)`
turns. The engine keeps one history per agent, asks the next agent for a
free-text turn, and gives that reply to the other agent as a user message.
Modes:

- `backrooms`: two agents alternate, A first.
- `brainstorm`: one agent, nudged to continue from its own last turn.
- `critic_pair`: A proposes, B attacks and counter-proposes.

Templates set the system prompts: `cli` (the original framing), `fugue`
(two voices in counterpoint), `student` (teacher and a student who keeps
asking why). The subject, seeds, constraints and recalled ideas are appended to
both system prompts as a labelled problem block.

A conversation stops at the turn cap, on `^C^C` in a turn, or when the
wall-clock deadline passes before a turn.

### Countering attractors

- **Lenses.** Each variation draws two *different* angles from a fixed list
  using a generator seeded by `(config.seed, variation index)`, and gives one
  to each agent ("start from the failure mode", "invert the obvious assumption",
  …).
- **Heterogeneous pairs.** `models.agent_a` and `models.agent_b` may differ.
- **Novelty penalty.** Items near anything in `memory_namespace` score low on
  originality.

### Free text first, structure second

Turns are never format-constrained. After the conversation a formatter agent
calls the typed tool `submit_items` with `{"items": [<schema>…]}`; each item is
validated and failures are returned for up to `budget.repair_attempts` repairs.
Rationale: *Let Me Speak Freely?* (arXiv 2408.02442) finds strict format
constraints degrade reasoning, and that reasoning first, then formatting
recovers most of the loss. See
[ADR 0003](../adr/0003-free-text-then-structure.md).

## Invariants and constraints

1. The engine owns the loop; a backend answers one turn and holds no loop.
2. The same `(request, backend)` produces the same lens draw.
3. Variations are independent and run concurrently.
4. The engine links no async timer; backends time out their own calls.

## Acceptance criteria

- Unit tests pin: alternation and history shape, `^C^C` stop, `max_turns` cap,
  deadline stop, brainstorm self-continuation, per-agent model routing, lens
  distinctness and reproducibility, and every mode/template having a distinct
  prompt.

## Open questions

- Whether a dedicated "ending" turn (Infinite Backrooms often closes with a
  ritual farewell) improves extraction.
