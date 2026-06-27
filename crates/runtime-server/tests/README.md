# `crates/runtime-server/tests/`

Integration tests for the `zagens-cli` runtime crate (package name `zagens-cli`, binary `zagens`).
Per `CONTRIBUTING.md`, each crate's integration tests live in its own `tests/` directory;
the repository-root `tests/` directory is unused.

## Mock LLM client (`integration_mock_llm.rs`)

`crates/runtime-server/src/llm_client/mock.rs` provides a `MockLlmClient` that implements
the `LlmClient` trait by replaying queue-driven canned responses and capturing
every outgoing `MessageRequest`. Tests mock at the **trait boundary** — never
at the `reqwest` HTTP layer — because the trait is the durable abstraction the
runtime is meant to depend on.

Coverage today exercises the trait surface end-to-end:

- streaming turn loop
- reasoning-content replay across tool-call rounds (V4 §5.1.1, the bug that
  broke v0.4.9-v0.5.1)
- tool-call round-trip with chunked input JSON
- multi-tool-call ordering inside a single turn
- compaction-style non-streaming `create_message`
- sub-agent style independent parent/child mocks
- capacity-gate observation of a captured request before stream drain

Full-engine mock LLM tests live in `crates/runtime-server/src/core/engine/tests.rs`
(`engine_llm_client_override_runs_mock_turn`, compaction, parallel read-only
tools) via `EngineConfig::llm_client_override` (A5.2 / A5.3).

## `--record` mode for `zagens eval`

The offline `zagens eval` harness now accepts `--record <DIR>`. When set,
each tool step appends one JSON Lines record to `<DIR>/<scenario>.jsonl`
(default scenario: `offline-tool-loop.jsonl`). Each line is a self-contained
JSON object with the schema:

```json
{ "request":  { "step": "list_dir", "kind": "List" },
  "response_events": [ { "type": "ok", "output": "…" } ] }
```

The mock LLM client (`crate::llm_client::mock`) replays these fixtures by
mapping each `response_events` array onto a canned `Vec<StreamEvent>`. Drop
generated fixtures into `crates/runtime-server/tests/fixtures/` so they ride the repo and
feed the mock in CI.

Quick example:

```bash
cargo run -p zagens-cli --bin zagens -- eval --record crates/runtime-server/tests/fixtures
cat crates/runtime-server/tests/fixtures/offline-tool-loop.jsonl | jq .
```

The scenario name is sanitized to `[A-Za-z0-9_-]` before forming the filename,
so unusual scenario strings stay portable across platforms.
