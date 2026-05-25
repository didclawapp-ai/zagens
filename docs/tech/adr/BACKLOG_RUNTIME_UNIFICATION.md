# Backlog ADR — Unify production HTTP with `core::Runtime`

**Status:** Proposed  
**Related:** §11.0 `handle_thread` / `ThreadMessageTurnPort`

## Context

- Production: `RuntimeThreadManager` + `deepseek-tui` HTTP.
- `core::Runtime::handle_thread` without `ThreadMessageTurnPort` returns `"queued"`.
- `app-server` uses `ThreadMessageTurnPort` with a simplified LLM path.

## Decision (draft)

Do **not** route Zagens HTTP through `core::Runtime` until `RuntimeThreadManager` delegates turn lifecycle into core without duplicating JSONL broadcast semantics.

## Acceptance

- Single turn entry for HTTP + app-server experiments.
- No second message pipeline for desktop.
