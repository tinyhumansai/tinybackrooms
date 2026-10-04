# Generate API

## Status and owner

Implemented (OpenHuman backend pending, see
[`../plans/openhuman-adapter.md`](../plans/openhuman-adapter.md)). Owner: tinyhumans.

## Problem

We want to generate creative ideas on demand, in a shape the caller chose, with
a knob for how much to explore. The ideas come from agents conversing; the
caller should not have to know that. The interface is one operation: **context
in, output spec, config → ranked typed items out.**

## Goals and non-goals

Goals:

- One request type, `GenerateRequest { input, output, config }`, and one
  response type, `GenerateResponse`, used identically by the library and the
  HTTP API.
- Every returned item satisfies the caller's JSON Schema. The schema is enforced
  by the engine, not trusted to the backend.
- Breadth (`variations`) and depth (`generations`) are independent knobs.
- A run is reproducible against a fixed backend: same request, same ranking.
- Partial failure degrades with warnings instead of failing the run.

Non-goals:

- Training or fine-tuning on outputs (Truth Terminal's pipeline). Accepted items
  are stored to memory for novelty penalties, not for fine-tuning.
- Human approval loops. The response is the curation point; a caller can add one.
- Guaranteeing creativity. The scores are proxies; see [`scoring.md`](scoring.md).

## Proposed behavior

### Request

```jsonc
{
  "input": {
    "brief": "…",                       // required, non-blank
    "seeds": [{"kind": "text|reference|prior_idea", "text": "…"}],
    "constraints": {"must": ["…"], "must_not": ["…"]},
    "memory_namespace": "optional"      // recall + store across runs
  },
  "output": {
    "item_name": "idea",                // used in prompts
    "item_guidance": "optional extra instruction for the formatter",
    "schema": { /* JSON Schema for ONE item */ }
  },
  "config": {                           // every field optional
    "variations": 3,                    // 1..=64 parallel conversations
    "generations": 6,                   // turns per conversation, both agents
    "items_per_variation": 3,
    "mode": "backrooms|brainstorm|critic_pair",
    "template": "cli|fugue|student",
    "temperature": 1.0,
    "models": {"agent_a": null, "agent_b": null, "formatter": null, "judge": null},
    "scoring": {
      "scorers": [{"kind": "originality|elaboration|judge", "weight": 1.0}],
      "select_top_k": null, "judge_raters": 3, "class_threshold": 0.8
    },
    "budget": {"max_turns": null, "wall_clock_secs": null, "repair_attempts": 2},
    "seed": 0,
    "include_transcripts": false
  }
}
```

Unknown fields are rejected. An empty `scorers` list means originality
(weight 1) and elaboration (weight 0.5).

### Response

`items` (ranked best first, each `{value, run_id, scores, total, boden_class?}`),
`diversity` (`vendi`, `distinct_classes`, `fluency`, `flexibility`, `dat`),
`transcripts` (only when requested), `usage`, `warnings`.

### Endpoints

| Method | Path | Behavior |
| --- | --- | --- |
| `GET` | `/healthz` | `200 {"status":"ok"}` |
| `POST` | `/v1/generate` | Request body → `200` response body |
| `POST` | `/v1/generate/stream` | Same request → SSE: one event per progress event, then `result` or `error` |

Errors use `{"error": {"code", "message"}}`:

| Status | `code` | Cause |
| --- | --- | --- |
| 400 | `invalid_json` | Body is not a valid request (including unknown fields) |
| 400 | `invalid_request` | Blank brief, zero/too many variations or generations, bad weights, invalid schema |
| 422 | `no_valid_items` | The run completed but no item satisfied the schema |
| 502 | `backend_error` | Every variation failed on the model or memory backend |

A stream validates the request before opening, so a bad request is a plain
`400`. Failures after the stream opens arrive as an `error` event. Closing the
connection cancels the run.

## Invariants and constraints

1. Validation happens before any backend call.
2. A returned item always satisfies `output.schema`.
3. A failed variation, a failed memory call and a failed judge rater are
   warnings. Only "every variation failed" and "no valid item" are errors.
4. Ranking is stable: equal totals keep variation order.
5. The wall-clock budget is checked between turns; a backend enforces its own
   per-turn timeout.
6. Schemas that use `$defs` / `definitions` keep working when nested for the
   tool call, because both are copied to the tool schema root.

## Acceptance criteria

- `cargo test --all-features` passes, including the pinned request and response
  wire forms and the route tests for every status code above.
- `examples/` runs every bundled request offline and each returns items.
- The binary starts, answers `/healthz`, serves `/v1/generate`, and exits 0 on
  `SIGINT`.

## Open questions

- Should `items_per_variation` have an upper bound? Today only `variations` is
  capped (64); a large `items_per_variation` is bounded only by the backend.
- Authentication and rate limiting are out of scope for v1; deploy behind a
  gateway.
