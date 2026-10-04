# Roadmap

## Shipped

- the engine (`crates/tinybackrooms`): validated requests, the Infinite-Backrooms
  conversation loop with three modes and three templates, typed extraction with
  a repair loop, scorers (originality, elaboration, multi-rater judge), and
  set-level diversity (Vendi, distinct classes, fluency, flexibility, DAT)
- the HTTP API (`crates/tinybackrooms-api`): `/v1/generate`, an SSE stream, and
  `/healthz`, with a credential-free demo backend
- the OpenHuman backend (`crates/tinybackrooms-openhuman`): one agent per seat,
  typed output through a host tool, selected with `TINYBACKROOMS_BACKEND=openhuman`
- `examples/`: four request files and an offline runner
- CI: format, clippy, build, test, per-file coverage, rustdoc, MSRV, supply
  chain, an engine-purity check, and the examples

## Next

- **OpenHuman-backed memory**: agents already run on OpenHuman; recall and
  storage still use the engine's process-local `Memory`
  ([plan](docs/plans/openhuman-adapter.md#memory-not-done)).
- A live run against a real provider, to judge output quality; the tests use
  fake models.
- A learned `Embedder`, so `originality` and `vendi` measure meaning rather than
  surface overlap.
- Calibrate default scorer weights against a small human-labelled set.
- A Jev-backed `judge` scorer, if its vendor claims hold up on our own set
  ([ADR 0004](docs/adr/0004-jev-as-optional-judge.md)).

## Out Of Scope

- fine-tuning on accepted items (the Truth Terminal pipeline)
- authentication and rate limiting: deploy behind a gateway
- anything that cannot be tested deterministically
