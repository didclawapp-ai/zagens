# Persistence layout (D7 SSOT)

**Status:** Landed 2026-05-26 · maintainer detail: `doc_Private/docs/tech/adr/D7_PERSISTENCE_UNIFICATION.md`

Zagens and the **`deepseek-runtime`** sidecar production data paths. **Not** a single physical database: Sessions and Runtime threads each use SQLite (or JSON fallback), linked by **`runtime_thread_id`**.

## Directories and files

| Track | Default path | Primary file | Environment variables |
|-------|--------------|--------------|------------------------|
| **Sessions** | `~/.deepseek/sessions/` | `sessions.db` (WAL); fallback `{id}.json` | — |
| **Runtime threads** | `~/.deepseek/tasks/runtime/` | `runtime.db`; fallback `threads/`, `events/*.jsonl` | `DEEPSEEK_RUNTIME_DIR` overrides runtime root; `DEEPSEEK_TASKS_DIR` overrides tasks root (default `~/.deepseek/tasks`) |

When `DEEPSEEK_RUNTIME_DIR` is set, it becomes the runtime store root; otherwise `{tasks_dir}/runtime`, where `tasks_dir` = `DEEPSEEK_TASKS_DIR` or `~/.deepseek/tasks`.

**SSOT narrative (D15):** Thread + Event (`runtime.db`) is authoritative for execution and replay; Session (`sessions.db`) is a **projection** for the desktop sidebar, linked via `runtime_thread_id`. Do not grow new sessions without a backing thread.

## HTTP contract (who writes / who reads)

| Operation | Write | Read |
|-----------|-------|------|
| Sidebar session list | — | `GET /v1/sessions` → SessionManager |
| Save session snapshot | `POST /v1/threads/{id}/persist-session` | SessionManager (includes **`runtime_thread_id`**) |
| Resume session | `POST /v1/sessions/{id}/resume-thread` | If `runtime_thread_id` and runtime has events → **reuse** thread; else create + seed |
| Thread / SSE | `POST /v1/threads`, `/turns`, events | RuntimeThreadStore |

## Link field

`SessionMetadata.runtime_thread_id` (SQLite column `runtime_thread_id`, D7 C1) points to `ThreadRecord.id`. Desktop **replay tool cards / thinking** depend on this link; missing link causes resume to seed a new thread.

## Kernel event log (Kernel V3)

Since **2026-06-16**, turn semantics are also recorded in an **append-only** table inside `sessions.db`:

| Table | Writer | Reader | Notes |
|-------|--------|--------|-------|
| **`kernel_events`** | `KernelEventWriter` during live turns (via `V3TurnHost`) | `ReplayTurnMachine`, resume repair, thread replay API | JSON `KernelEvent` payloads; monotone `seq` |

Schema and projection rules: [AGENT_KERNEL_V3.md](./AGENT_KERNEL_V3.md) §2. Code: [`kernel_event_log.rs`](../../crates/runtime-adapters/src/persist/kernel_event_log.rs).

**Resume:** `[kernel] log_transcript_repair` defaults to **true** — transcript rebuild prefers the kernel log over session JSON snapshots. Session files remain a desktop projection/export format (D7/D15 unchanged at the HTTP boundary).

Golden parity fixtures: [`fixtures/harness/kernel-v3-replay/`](../../fixtures/harness/kernel-v3-replay/).

## Non-SSOT / removed

- **`crates/app-server`** — removed in D7; production HTTP is only `runtime_api` / `deepseek-runtime`
- **`deepseek-state` / `core::Runtime`** — removed in D15; Zagens is the sole entry point, no CLI legacy path
