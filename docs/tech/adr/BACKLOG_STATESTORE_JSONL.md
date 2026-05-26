# Backlog ADR — `StateStore` vs `runtime_threads` JSONL

**Status:** Superseded by [D7_PERSISTENCE_UNIFICATION.md](./D7_PERSISTENCE_UNIFICATION.md) (2026-05-26)  
**Related:** [PERSISTENCE.md](../PERSISTENCE.md)

## Context

- Sidecar HTTP: `RuntimeThreadStore` (`runtime.db`).
- CLI legacy: `deepseek-state` (`StateStore`); `deepseek thread list --source state`.

## Decision (landed)

No unified physical DB. Cross-store link is **`runtime_thread_id`** on SavedSession. CLI defaults to read-only runtime listing (D7 C4). StateStore remains for legacy CLI metadata writes only.
