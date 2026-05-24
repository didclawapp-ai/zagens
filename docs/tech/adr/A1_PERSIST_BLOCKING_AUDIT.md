# A1.3 — Runtime persist blocking I/O audit

**Status:** Accepted (2026-05-24)  
**Scope:** `crates/tui/src/runtime_threads/persist.rs`, `thread_store_sqlite.rs`, call sites

## Policy

| Path | Rule |
|------|------|
| `append_event` (SQLite) | `spawn_blocking` + `db.lock()` inside blocking task ✅ |
| `append_event` (JSONL) | `spawn_blocking` for state + JSONL ✅ |
| `thread_crud` / `monitor` saves | `spawn_blocking` ✅ |
| `events_since` (SQLite) | **Sync** — callers must be blocking or test-only |
| `events_since` (JSONL) | Sync file read — same |

## Follow-ups

- HTTP handlers that need `events_since` should use `spawn_blocking` wrapper — **done** (`events_since_async`).
- Live TUI tool isomorphism: `history_isomorphism::live_history_matches_messages` + `App::debug_assert_live_history_isomorphism` at turn end / tool complete / session load / backtrack.
