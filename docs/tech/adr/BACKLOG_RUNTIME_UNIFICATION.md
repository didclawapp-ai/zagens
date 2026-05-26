# Backlog ADR — Unify production HTTP with `core::Runtime`

**Status:** Superseded by [D7_PERSISTENCE_UNIFICATION.md](./D7_PERSISTENCE_UNIFICATION.md) (2026-05-26)  
**Related:** §11.0 `handle_thread` / `ThreadMessageTurnPort`

## Context

- Production: `RuntimeThreadManager` + sidecar HTTP (`runtime_api`).
- `core::Runtime::handle_thread` without `ThreadMessageTurnPort` returns `"queued"` (CLI/core experiments only).
- Former `app-server` path **removed** in D7.

## Decision (landed)

Production Zagens/TUI HTTP **does not** route through `core::Runtime` as the persistence SSOT. Sessions + Runtime threads are documented in [PERSISTENCE.md](../PERSISTENCE.md); linked by `runtime_thread_id`.

Unifying `core::Runtime` turn entry with HTTP remains a **future** spike if needed — not blocking §1 #6.
