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
- Live TUI tool isomorphism: `history_isomorphism::live_history_matches_messages` + `App::check_live_history_isomorphism` at turn end / tool complete / session load / backtrack.

## Status (2026-05-25 — A1 follow-up closed)

Live `ToolCell` vs `session.messages` isomorphism is now a **production-grade**
check, not a debug-only assert (roadmap §17.5 余项 #1):

| Build profile | Behavior on drift |
|---------------|------------------|
| **release** (user installs) | `tracing::warn!(target = "tui::history_isomorphism")` with `site` / `api_messages` / `history_cells` + bump `history_isomorphism::drift_count()` |
| **debug / tests** | Same warn + counter bump, **plus** `debug_assert!` so CI fails loudly |

**Surface:** `crates/tui/src/tui/history_isomorphism.rs::record_drift` /
`drift_count` (process-wide `AtomicU64`), called via
`App::check_live_history_isomorphism(site)` from the four live paths in
`crates/tui/src/tui/ui.rs` — `tool_complete`, `turn_complete`,
`session_load`, `backtrack`.

**Tests:** `record_drift_increments_global_counter`,
`reset_drift_count_for_test_zeroes_counter`,
`drift_is_detected_when_tool_output_diverges`
(`crates/tui/src/tui/history_isomorphism.rs` `#[cfg(test)]`); 90/90 history
tests pass (2026-05-25).
