# Backlog ADR — `StateStore` vs `runtime_threads` JSONL

**Status:** Proposed (P2 backlog)  
**Related:** [RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md) §11.0

## Context

- CLI / `app-server`: `deepseek-state` (`StateStore`).
- Sidecar HTTP: `RuntimeThreadStore` JSONL/SQLite under runtime data dir.

## Decision (draft)

No unified persistence in P2 follow-ups without a migration ADR and dual-write period.

## Options (for future spike)

1. JSONL as SSOT; StateStore as index.
2. SQLite runtime store absorbs thread list metadata only.
3. Deprecate StateStore for sidecar-only deployments.
