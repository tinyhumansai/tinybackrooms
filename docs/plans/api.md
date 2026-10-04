# Plan: HTTP API

Implements [`../specs/generate-api.md`](../specs/generate-api.md). Status:
complete.

1. `config/`: environment parsing through an injectable lookup.
2. `demo/`: `DemoConversant` and the schema sampler, so the server runs without
   credentials.
3. `routes/`: error mapping, `/healthz`, `/v1/generate`, then the SSE stream.
   Bodies are decoded by hand so every failure uses one error shape.
4. `lib.rs`: `serve` with graceful shutdown; `run_from_env` as the whole binary.
5. `tests/binary.rs`: spawn the real binary; also what gives `main.rs` coverage.
6. `examples/`: request files and the offline runner.

Verification: route tests for every status in the spec; the binary test for
startup, serving, bad config, a taken port and a clean exit on `SIGINT`.
