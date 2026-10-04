# Plan: OpenHuman backend

Status: **implemented** (agents and typed output); memory is not (see below).
Implements the `Conversant` port on OpenHuman agents.

## Decisions

- **A separate crate, `tinybackrooms-openhuman`, not a feature of the engine.**
  The first plan was a Cargo feature. The engine's purity check
  (`.github/scripts/assert-pure.sh`) runs with `--all-features` and forbids the
  harness, so a feature would have broken the guarantee it exists to give. The
  API crate selects the adapter through its own optional `openhuman` feature.
- **One agent per seat, identified by what it is.** The id hashes the role, the
  system prompt and, for a typed call, the tool and schema. Variations and
  requests that ask for the same thing share an agent handle.
- **A fresh session per turn, seeded with the engine's history** (`Turn::seed`).
  The engine owns the conversation, so OpenHuman keeps no state between turns
  and one handle can serve many turns at once (the tool factory receives the
  session id, which is how concurrent typed calls stay apart).
- **Typed output is a host tool** named by the request, with the requested schema
  as its parameters. Its arguments land in a slot keyed by session id. The embed
  facade has no `response_format`, so this is the supported route.
- **Chat agents have no tools** (`ToolScopeSpec::Named(vec![])`, one iteration).
  The default definition is the orchestrator with every tool visible.
- **A local backend stub.** OpenHuman makes non-inference calls to its backend,
  and signed out of the real one they hang. Bring-your-own-key runs point the
  runtime at a loopback server that answers every request with a success body.

## What running real agents showed

Found by reading the wire, not the docs. Each one changed the design.

- **OpenHuman wraps a persona around the system prompt.** An agent's system
  message is the caller's prompt followed by `SOUL.md`, `IDENTITY.md` and
  `ROLE.md`: "warm, direct, no filler", "never invent", "answer first", a
  tool-use protocol. That is the opposite of the backrooms framing, so it would
  quietly ruin the loop. **Fix:** every turn seeds an explicit `("system", prompt)`
  row first. A seeded system row replaces the whole built prompt, and the model
  sees exactly the seat's prompt.
- **Seeding without that row drops the system prompt entirely.** `Turn::seed`
  replaces the session's history, system message included.
- **A per-turn `.temperature()` is not applied;** every call went out at 0.4.
  The temperature is set on the agent definition instead, and is part of the
  agent's identity, so a different temperature is a different agent.
- **Built-in tools stay visible.** `ToolScopeSpec::Named` still leaves the
  `juice_*` tools on a typed-output agent's tool list next to `submit_items`.
  Chat agents show no tools. A model that calls a `juice_*` tool instead of
  `submit_items` ends as "the agent never called `submit_items`".
- **`Current Date & Time: ...` is prefixed to each final user message.** Mild
  noise in a loop that is otherwise carefully framed; not configurable from the
  embed facade.
- **Provider errors are retried with backoff.** A 5xx can surface as a turn
  timeout rather than a prompt failure; a 4xx fails fast.

## Constraints

- One `Runtime` per process; a second `OpenHumanConversant::new` fails.
- Tokio worker stacks of 16 MiB. `tinybackrooms-api` builds its runtime that way.
- Per-turn timeouts live in the adapter. Dropping a timed-out turn is the only
  cancellation the facade offers.
- `openhuman-embed` needs Rust 1.96, so the workspace `rust-version` is 1.96.

## Memory: not done

The embed facade has no typed host memory API. Recall and storage stay on the
engine's `Memory` port (`InMemoryMemory` by default, so memory is
process-local). Reaching OpenHuman's own memory would mean building a `Core` and
calling `openhuman.memory_doc_put` / `memory_recall_memories` over RPC, or
giving agents the memory tool and a namespace. Neither has been tried; the
open questions are whether memory is isolated per agent or only per namespace,
and what the embedding default is when none is configured.

## Verification

```sh
cargo test -p tinybackrooms-openhuman
cargo test -p tinybackrooms-api --features openhuman
.github/scripts/assert-openhuman-pin.sh
```

The adapter tests run real OpenHuman agents against a fake OpenAI-compatible
provider: history and the system prompt reach the model, the typed tool's schema
is on the wire and its arguments come back, concurrent typed calls do not cross,
and a missing tool call, a provider error, a timeout and a second runtime are
each reported as errors.

A live run against a real provider is a manual step:

```sh
TINYBACKROOMS_BACKEND=openhuman TINYBACKROOMS_OPENHUMAN_API_KEY=... \
  cargo run -p tinybackrooms-api --features openhuman
```
