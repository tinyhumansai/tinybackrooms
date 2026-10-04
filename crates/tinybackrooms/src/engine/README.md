# engine

`Engine::generate`: the whole pipeline from request to response.

## Pipeline

1. **Validate** the request and compile the output schema. Nothing is spent on
   a request that cannot succeed.
2. **Recall** earlier items from `memory_namespace`, if set.
3. **Run every variation concurrently** with `join_all`: converse (`backrooms`),
   then extract (`extract`).
4. **Pool** the survivors, turning failures into warnings.
5. **Rank** (`score`), then **store** the selected items back to memory.

## Failure model

Partial failure degrades to warnings: a failed variation, a failed memory call
and a failed judge rater never fail the run on their own. Two outcomes are
errors: every variation failed (the first backend error is returned), and the
variations ran but no item satisfied the schema (`Error::NoValidItems`).

## Ports

`Engine` holds `Arc`s of the `Conversant`, `Memory`, `Embedder` and `Observer`
it was built with, so cloning is cheap. A server holds one engine and gives each
request a clone with its own `Observer`:

```rust
let per_request = engine.clone().with_observer(my_sink);
```

## Constraints

- The engine links no async runtime and no timer. The wall-clock budget is
  checked between conversation turns; a backend enforces its own per-turn
  timeout.
- Dropping the `generate` future cancels the run. That is how the HTTP stream
  stops spending model calls when a client disconnects.
