# Examples

A standalone cargo workspace. It is deliberately not a member of the root
workspace, so its dependencies never enter the normal build, and it has its own
`Cargo.lock`.

## Request files

Each file in [`requests/`](requests/) is a complete `POST /v1/generate` body.

| File | Shows |
| --- | --- |
| `product-naming.json` | The default `backrooms` mode with the `cli` template, seeds of every kind, constraints, weighted scorers and `select_top_k` |
| `story-premises.json` | `critic_pair` mode, a different model per seat, the `judge` scorer, a budget, and transcripts in the response |
| `research-hypotheses.json` | `brainstorm` mode, an enum field in the schema, and a `memory_namespace` so a second run avoids repeating the first |
| `game-mechanics.json` | Six variations in the `fugue` template, array and length constraints, and a tighter `class_threshold` |

## Run them offline

```sh
cargo run --manifest-path examples/Cargo.toml --bin offline
cargo run --manifest-path examples/Cargo.toml --bin offline -- examples/requests/story-premises.json
```

The offline backend fabricates schema-valid output, so this shows the request
and response shapes and the scoring plumbing, not creativity.

## Run them against the server

```sh
cargo run -p tinybackrooms-api &
curl -s localhost:8080/v1/generate -H 'content-type: application/json' \
  -d @examples/requests/product-naming.json
curl -sN localhost:8080/v1/generate/stream -H 'content-type: application/json' \
  -d @examples/requests/story-premises.json
```

## Tests

```sh
cargo test --manifest-path examples/Cargo.toml
```

Every request file must parse, validate and return items offline.
