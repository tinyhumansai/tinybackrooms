# tinybackrooms

A configurable creativity engine, and an HTTP API around it.

You give it **context**, a **spec for the output**, and a **config**. It has
agents talk to each other in the style of Andy Ayrey's Infinite Backrooms, turns
what they said into typed items, scores them for novelty, and returns the best.

```text
input  ──►  conversations (free text)  ──►  typed extraction  ──►  scoring  ──►  ranked items
output spec   N variations × M turns        your JSON Schema,       originality,    + diversity
config        two agents, no task           validated + repaired    elaboration,    + warnings
                                                                    multi-rater judge
```

## Why a conversation

Instruction-tuned models answer in an assistant register and converge on the
same ideas. Infinite Backrooms put two Claude instances in a command-line
simulator with no task, each answering the other, and the pair drifted somewhere
neither would go alone. This engine keeps that loop, then counters its known
failure mode (two copies of one model fall into the same attractors) with a
different angle per agent, optional different models per seat, and a penalty for
repeating earlier runs. The research behind each choice is in
[`docs/specs/`](docs/specs/README.md).

Ideas come from unconstrained text; structure is applied afterwards by a second
agent, because constraining a model to a format while it is still thinking costs
it quality ([ADR 0003](docs/adr/0003-free-text-then-structure.md)).

## Quick start

```sh
cargo run -p tinybackrooms-api     # listens on 127.0.0.1:8080
```

The default `demo` backend fabricates schema-valid output so the server runs
without credentials. It demonstrates the interface, not creativity. For real
ideas, run OpenHuman agents against any OpenAI-compatible provider:

```sh
TINYBACKROOMS_BACKEND=openhuman \
TINYBACKROOMS_OPENHUMAN_API_KEY=... \
  cargo run -p tinybackrooms-api --features openhuman
```

Setting names and defaults are in [`.env.example`](.env.example).

```sh
curl -s localhost:8080/v1/generate -H 'content-type: application/json' -d '{
  "input":  {"brief": "Name a coffee shop inside a decommissioned lighthouse."},
  "output": {"item_name": "shop name", "schema": {
    "type": "object", "required": ["name"],
    "properties": {"name": {"type": "string"}}}},
  "config": {"variations": 3, "generations": 6, "items_per_variation": 2}
}'
```

Stream progress instead with `POST /v1/generate/stream` (server-sent events:
`variation_started`, `turn`, `extracted`, `scored`, then `result`).

## The request

| Part | What it holds |
| --- | --- |
| `input` | `brief`, `seeds` (text / reference / prior idea), `constraints` (must / must not), optional `memory_namespace` |
| `output` | `item_name`, `item_guidance`, and a JSON Schema every item must satisfy |
| `config` | `variations`, `generations`, `items_per_variation`, `mode`, `template`, `temperature`, per-role `models`, `scoring`, `budget`, `seed`, `include_transcripts` |

Every `config` field is optional. The full contract, status codes and
invariants are in [`docs/specs/generate-api.md`](docs/specs/generate-api.md);
runnable requests are in [`examples/`](examples/README.md).

| Config knob | Effect |
| --- | --- |
| `variations` | Independent conversations in parallel: breadth |
| `generations` | Turns per conversation: depth |
| `mode` | `backrooms` (two agents), `brainstorm` (one), `critic_pair` (propose and attack) |
| `template` | `cli` (the original), `fugue`, `student` |
| `scoring.scorers` | `originality`, `elaboration`, `judge`, each weighted |
| `budget` | `max_turns`, `wall_clock_secs`, `repair_attempts` |

## The response

Ranked `items`, each with its `value` (valid against your schema), `run_id`,
per-scorer `scores`, `total` and, when judged, a Boden creativity class. Plus
set-level `diversity` (Vendi score, distinct classes, fluency, flexibility,
DAT), `usage`, and `warnings` for anything dropped along the way. A failed
variation or judge call degrades to a warning rather than failing the run.

## As a library

```rust
let engine = Engine::new(Arc::new(my_backend));   // any `Conversant`
let response = engine.generate(&request).await?;
```

`crates/tinybackrooms` holds all the logic and reaches models, memory and
embeddings only through the `Conversant`, `Memory` and `Embedder` ports, so the
whole pipeline is tested without a network (`ScriptedConversant`).

## Workspace

```text
crates/tinybackrooms/       the engine: requests, conversations, extraction, scoring
crates/tinybackrooms-openhuman/   the engine's model port on OpenHuman agents
crates/tinybackrooms-api/   the axum server and the credential-free demo backend
vendor/openhuman/           pinned OpenHuman submodule (git submodule update --init --recursive --depth 1)
examples/                   standalone workspace: request files and an offline runner
docs/                       specs, plans and decision records
```

## Status

The engine, the API, the examples and the OpenHuman agent backend are in place.
Not done: OpenHuman-backed *memory*, so `memory_namespace` recall is
process-local for now. Run it against a real provider yourself before trusting
the output quality; the tests use fake models. See
[`docs/plans/openhuman-adapter.md`](docs/plans/openhuman-adapter.md) and
[`ROADMAP.md`](ROADMAP.md).

## Development

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --all-targets --all-features
cargo test --all-features
cargo run --manifest-path examples/Cargo.toml --bin offline
```

See [`CONTRIBUTING.md`](CONTRIBUTING.md) and [`AGENTS.md`](AGENTS.md). Licensed
GPL-3.0-only.
