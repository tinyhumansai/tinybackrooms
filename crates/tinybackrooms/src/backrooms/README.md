# backrooms

The Infinite-Backrooms conversation loop: one *variation* is one conversation.

## Design

The engine owns the loop. It keeps one history per agent, asks the next agent
for a free-text turn through the `Conversant` port, and hands that reply to the
other agent as a **user** message. No turn is format-constrained; structure is
applied later by `extract`.

- `mod.rs`: `run`, the loop. Stops on the turn cap
  (`min(generations, budget.max_turns)`), on `^C^C` in a turn, or when the
  deadline has passed before a turn.
- `template.rs`: the prompts (`cli`, `fugue`, `student`, plus the brainstorm and
  critic roles), the problem block (brief, seeds, constraints, recalled ideas),
  and the fixed list of *lenses*.
- `types.rs`: `Turn`, `StopReason`, `Transcript` (and `Transcript::render`, the
  text the formatter reads).

## Behavior worth knowing

- Each variation draws two **different** lenses from a generator seeded by
  `(config.seed, variation index)`, one per agent. The same model talking to
  itself otherwise settles into the same attractors.
- `brainstorm` mode uses one agent and nudges it to continue from its own last
  turn; `critic_pair` gives A a proposer role and B an attacker role.
- A failed turn fails the variation and discards its partial conversation; the
  engine turns that into a warning.

## Constraints

- No async timer: a backend times out its own turns. The deadline is checked
  only between turns.
- The public surface is `Transcript`, `Turn` and `StopReason`; `run` is
  crate-private and reached through `Engine::generate`.
