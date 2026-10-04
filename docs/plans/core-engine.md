# Plan: core engine

Implements [`../specs/backrooms-engine.md`](../specs/backrooms-engine.md) and
[`../specs/scoring.md`](../specs/scoring.md). Status: complete.

Order (each step: failing test, then code):

1. `request/`: wire types, defaults, `validate()`, schema compilation.
2. `conversant/`: the `Conversant` port and `ScriptedConversant`.
3. `embed/`, `rng/`, `memory/`: the supporting ports and a seeded generator.
4. `backrooms/`: prompts, lens draw, the turn loop with its three stop reasons.
5. `extract/`: the `submit_items` tool schema, validation, bounded repair.
6. `score/`: scorers, judge, Vendi and the other set metrics, ranking.
7. `engine/`: validate, recall, concurrent variations, pool, rank, store.
8. `lib.rs` crate docs with a runnable example; `tests/generate.rs` against the
   public API.

Verification:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
.github/scripts/check-file-coverage.sh 90 target/coverage.json
```
