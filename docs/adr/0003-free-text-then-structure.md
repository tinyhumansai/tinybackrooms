# 3. Free text first, structure second

- **Status:** Accepted
- **Date:** 2026-10-04

## Context

We need typed, schema-valid output that is also creative. Constrained decoding
(Outlines, arXiv 2307.09702; XGrammar, arXiv 2411.15100; SGLang's jump-forward
decoding) makes typed output fast by masking invalid tokens and skipping
deterministic ones, but it needs logit access or a provider that implements it.
Separately, *Let Me Speak Freely?* (arXiv 2408.02442) finds strict format
constraints degrade reasoning, with the loss largely recovered by reasoning
freely and formatting in a second step.

The service calls hosted models through the OpenHuman embed facade, which
exposes no `response_format` and no logits; `reply` is a string. It does support
host tools with JSON-schema parameters.

## Decision

Two phases per variation. The conversation is unconstrained free text. A
separate formatter call then reads the transcript and answers by calling one
typed tool, `submit_items`, whose arguments are `{"items": [<caller schema>]}`.
The engine validates every item against the caller's schema itself and returns
failures to the formatter for a bounded number of repair attempts.

## Consequences

- Exploration is never format-constrained.
- Typed output works on any backend that supports tool calls; a backend with
  native structured output can use it to make the formatter faster without
  changing the engine.
- The formatter is one extra call per variation. Speed comes from running
  variations concurrently.
- Root-relative `$ref`s in the caller's schema survive nesting only because
  `$defs` and `definitions` are copied to the tool schema root.
