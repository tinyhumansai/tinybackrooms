# 2. HTTP transport, and an engine behind ports

- **Status:** Accepted
- **Date:** 2026-10-04

## Context

The repository started as a template for a TinyBus cdylib module: a contract
crate, a module crate, a release workflow that builds native packages, and a
vendored `tinybus` submodule. The product is an API that takes a creative brief
and returns typed ideas, which callers reach over the network. Its logic must
also run offline in tests, because the model calls are nondeterministic and
cost money.

The sibling `tinyhivemind` repository has no API crate to copy; what it does
establish is a pure core behind ports with the harness linked only in an
adapter, and examples kept in standalone workspaces.

## Decision

- Serve **HTTP with axum** from `crates/tinybackrooms-api`. Remove the TinyBus
  module scaffolding, the `template-bus` contract crate, the module release
  workflow and the `vendor/tinybus` submodule.
- Keep **all logic in `crates/tinybackrooms`**, reaching the outside world only
  through three ports: `Conversant` (models), `Memory` (recall and store) and
  `Embedder` (vectors), plus an `Observer` for progress. The engine links no
  async runtime and no HTTP client.
- Request and response types live in the engine crate. The API crate is their
  only consumer, so a separate contract crate would be a layer with nothing to
  protect.
- Examples live in `examples/`, a standalone workspace.

## Consequences

- The whole pipeline is tested with `ScriptedConversant`, with no network.
- A model backend is one trait implementation; the API crate and the engine do
  not change when it does.
- Per-turn timeouts belong to backends, since the engine has no timer.
- If a TinyBus surface is wanted later it is a new adapter crate over the same
  engine, not a rewrite. A contract crate can be split out then.
- The release workflow is gone. Nothing here is published; consumers pin a git
  commit, as with tinyhivemind.
