# extract

Turns a finished transcript into schema-valid items.

## Design

A formatter agent reads the rendered transcript and answers by calling one typed
tool, `submit_items`, whose arguments are `{"items": [<caller schema>, ...]}`
with `maxItems = items_per_variation`. The caller's schema is the only source of
truth: **every item is validated here** with `jsonschema`, whatever the backend
claims to enforce.

Items that fail are returned to the formatter, with the validator's message and
the previous answer, for up to `budget.repair_attempts` more calls. Valid items
accumulate across attempts and exact duplicates are dropped.

## Behavior worth knowing

- `$defs` and `definitions` in the caller's schema are copied to the tool
  schema's root, so root-relative `$ref`s still resolve once the schema is
  nested under `items`.
- `Extraction::invalid` counts items still failing after the last attempt; it is
  `0` when the requested count was reached. The engine turns a nonzero count
  into a warning.
- An answer with no `items` array counts as one rejected item.

## Why a separate pass

Constraining a model to a format while it is still thinking costs it quality
(*Let Me Speak Freely?*, arXiv 2408.02442). See
`docs/adr/0003-free-text-then-structure.md`.
