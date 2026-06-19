# Zagens API Design

> **Zagens shell version:** 0.7.5 (`crates/desktop/Cargo.toml`) | **Document revision:** 2026-06-12 | **Authoritative implementation:** `commands.rs`, `runtime_proxy.rs`, `runtime_api/router.rs` `build_router`, `web-ui/src/api/client.ts`

This document describes the **dual-channel integration API** of the **Zagens desktop shell**. Protocol types live in `crates/protocol/`; the HTTP routing SSOT is `crates/runtime-server/src/runtime_api/router.rs` (sidecar: **`deepseek-runtime`**).

**OpenAPI 3.1 (D8):** Checked-in contract [`openapi/zagens-runtime-v1.openapi.json`](./openapi/zagens-runtime-v1.openapi.json); exported by `export-runtime-openapi` from Rust `schemars` + path table; `web-ui` TS generated via `openapi-typescript` (see [`adr/D8_OPENAPI_TS_GENERATION.md`](./adr/D8_OPENAPI_TS_GENERATION.md)). Legacy `RUNTIME_API.md` has been removed. Runtime architecture diagrams: [RUNTIME_ARCHITECTURE.md](./RUNTIME_ARCHITECTURE.md).

**Agent behavior:** The runtime prompt loaded by the same sidecar includes hallucination-control sub-rules (Capability / Architecture Claims) in `crates/runtime-server/src/prompts/base.md`. **SSE diagrams and capability claims in this document are illustrative only** — integrate against code and `streamNormalize.ts`.

> **Only production HTTP runtime:** Zagens and headless scripts use `crates/runtime-server/src/runtime_api/` (`/v1/*`); binary is **`deepseek-runtime`**. ~~`crates/app-server`~~ **removed in D7**; ~~`deepseek-tui` / `deepseek serve`~~ **removed in D6 Phase B** (see [`adr/D4_APPSERVER_DEPRECATED.md`](./adr/D4_APPSERVER_DEPRECATED.md) · [`adr/D6_PHASE_B_CLI_SUNSET.md`](./adr/D6_PHASE_B_CLI_SUNSET.md)).

---

## 1. Architecture Overview

Zagens uses a **dual-channel API** architecture: the WebView frontend and Rust backend communicate over two paths.

```
┌────────────────────────────────────────────────────────────┐
│                    Zagens (Tauri App)                      │
│                                                            │
│  ┌─────────────────────┐    ┌────────────────────────┐     │
│  │   WebView (React)   │    │    Rust Shell          │     │
│  │                     │    │                        │     │
│  │  Channel A:         │    │  commands.rs           │     │
│  │  Tauri invoke() ────┼───►│  runtime_proxy.rs      │     │
│  │  (native bridge,    │    │  sidecar.rs            │     │
│  │   ~64 commands)     │    │  main.rs               │     │
│  │                     │    │  ┌──────────────────┐  │     │
│  │  Channel B:         │    │  │  Sidecar Process │  │     │
│  │  via proxy or dev   │    │  │  deepseek-runtime│  │     │
│  │  fetch (127.0.0.1)  │───►│  │  (HTTP + SSE)    │  │     │
│  │  73 HTTP routes*    │    │  └──────────────────┘  │     │
│  └─────────────────────┘    └────────────────────────┘     │
└────────────────────────────────────────────────────────────┘
```

| Channel | Transport | Purpose |
|---------|-----------|---------|
| **A — Tauri IPC** | `invoke()` → Rust `#[tauri::command]` | System-level ops: secrets, settings, binary I/O, **runtime HTTP/SSE proxy (H06)**, terminal, multi-window |
| **B — Runtime HTTP** | Under Tauri via `runtime_proxy`; Vite dev connects directly to `http://127.0.0.1:{port}` | Agent runtime: chat threads, sessions, streaming, MCP, tasks, usage |

\* §8 “Channel B” table counts **73 rows** by **method + path** (including `/health` and `/internal/probe`); verify against `runtime_api/router.rs` + `runtime-api/src/router.rs`.

**vs CLI / `app-server`:** Zagens **does not** start `crates/app-server`; the desktop only spawns the bundled `deepseek-runtime` sidecar; the HTTP surface is `runtime_api/`. `app-server` is for other entry points (if any), not the Zagens data path.

---

## 2. Tauri IPC Commands (Channel A)

**Protocol:** JSON-RPC (Tauri built-in), invoked via `@tauri-apps/api/core` `invoke()`.
**Authentication:** No extra auth — in-process method calls; security enforced by Tauri sandbox.

### 2.1 Runtime Connection

| Command | Parameters | Return | Description |
|---------|------------|--------|-------------|
| `get_runtime_port` | — | `u16` | Sidecar listen port (default 7878) |
| `runtime_http` | `request: { method, path, body? }` | `{ status, body }` | **H06 proxy:** REST `/v1/*`; Rust injects Bearer |
| `runtime_post_stream` | `body: String` | `()` + SSE events | **H06 proxy:** `POST /v1/stream`; events pushed to WebView via Tauri |
| `runtime_get_sse` | `path: String`, `thread_id?: String` | `()` + SSE events | **H06 proxy:** `GET …/events` and other SSE. `thread_id` (multi-session P0.1) isolates the consumer per `(window, thread)`; omit for legacy per-window behaviour |
| `runtime_cancel_sse` | `thread_id?: String` | `()` | Cancel in-flight `runtime_get_sse`. With `thread_id`: cancel only that thread's consumer; without: cancel every in-flight SSE for the window (legacy global Stop) |
| `get_platform_info` | — | `PlatformInfo` | `{ os, arch, version }` — `version` is **Zagens shell** SemVer (`CARGO_PKG_VERSION`), not OS version |
| `get_os_theme` | — | `String` | **Placeholder:** currently fixed `"dark"`; system theme API not read |
| `get_locale` | — | `String` | Read persisted app locale from `~/.deepseek/config.toml` (`zagens-config`) |
| `set_app_locale` | `locale: String` | `()` | Persist app locale to config |
| `restart_sidecar` | — | `()` | Trigger sidecar restart (reload config.toml) |

#### 2.1.1 Cancel / Interrupt Two-Layer Contract (D9)

Desktop **Stop** / Escape must execute both layers; disconnecting the stream alone does not stop the runtime turn loop.

| Layer | Mechanism | Effect | If skipped |
|-------|-----------|--------|------------|
| **Layer 1** | `AbortSignal` + IPC `runtime_cancel_sse` | Disconnect WebView ↔ sidecar SSE / poll pipe | Turn still running; LLM / tools continue |
| **Layer 2** | HTTP `POST /v1/threads/{id}/turns/{turn_id}/interrupt` | runtime `Op::Interrupt` → `CancellationToken` | Local UI still shows streaming |

**Web UI SSOT:** `crates/desktop/web-ui/src/api/turnControl.ts`

- `disconnectThreadEventStream(signal?)` — Layer 1 only (navigation leave, sidecar restart, etc.)
- `stopThreadTurn({ threadId, turnId, streamControl? })` — **user Stop:** Layer 2 then Layer 1 (ignore 409 if already finished)

Runtime narrative and broadcast details: [RUNTIME_ARCHITECTURE.md](./RUNTIME_ARCHITECTURE.md) §8.

#### 2.1.2 Multi-Window Thread Ownership (D10)

| IPC | Description |
|-----|-------------|
| `register_window_thread { threadId }` | Current WebView declares UI ownership of thread |
| `thread_owned_by_window { threadId }` | Query whether this window is still owner |

**SSE filtering:** Non-owner windows must ignore live `GET …/events` / `runtime_get_sse` increments (`filterThreadStreamEvents` + `windowOwnsThreadForStream` TTL cache). Same for `approval.required`. Historical replay (session load) is not limited.

> **`get_runtime_token` removed (2026-05-20 H06 security follow-up):** Bearer no longer enters WebView; Tauri production path always goes through `runtime_proxy`. Full IPC list: `crates/desktop/src/main.rs` `generate_handler!` (**64** commands as of 2026-06-12, including sandbox, LHT, hooks, terminal, multi-window, and app updater).

### 2.2 API Key Management

| Command | Parameters | Return | Description |
|---------|------------|--------|-------------|
| `get_api_key_status` | — | `ApiKeyStatus { configured }` | Check if DeepSeek API Key is stored in OS keychain |
| `save_deepseek_api_key` | `key: String` | `()` | Write to OS keychain → clear plaintext in config.toml → restart sidecar |
| `clear_deepseek_api_key` | — | `()` | Remove key from keychain → restart sidecar |
| `get_vision_bridge_status` | — | `VisionBridgeStatus { configured, base_url, model }` | Vision transcription bridge status |
| `save_vision_bridge` | `api_key, base_url, model` | `()` | Save vision bridge config → restart sidecar |
| `clear_vision_bridge` | — | `()` | Clear vision bridge config → restart sidecar |
| `vision_transcribe_image` | `data_url: String` | `String` | Call vision model to transcribe image to text |

### 2.3 File / Workspace Operations

| Command | Parameters | Return | Description |
|---------|------------|--------|-------------|
| `read_thread_workspace_binary` | `thread_id, relative_path` | `BinaryFileResponse` | Read thread workspace binary (see structure below); Runtime `workspace/file` is **UTF-8 text only** |
| `read_workspace_binary_at_root` | `workspace_root, relative_path` | `BinaryFileResponse` | Same, without thread context |

**`BinaryFileResponse`** (`commands.rs`):

```json
{
  "mime_type": "image/png",
  "base64": "...",
  "size": 12345,
  "truncated": false
}
```

| `open_in_shell` | `path: String` | `()` | Open path in system terminal |
| `open_with_system_app` | `path: String` | `()` | Open file with system default app |
| `open_external_url` | `url: String` | `()` | Open HTTPS/HTTP URL in system browser |
| `export_thread_json` | `thread_id: String` | save dialog | Export thread conversation as JSON |
| `export_session_json` | `session_id: String` | save dialog | Export session record as JSON |

### 2.4 System Settings

| Command | Parameters | Return | Description |
|---------|------------|--------|-------------|
| `get_system_settings` | — | `SystemSettings` | Read full system settings |
| `save_system_settings` | `settings: SystemSettings` | `()` | Write settings → restart sidecar |

**`SystemSettings` structure:**

```
default_model: string      reasoning_effort: string      cost_currency: string
allow_shell: bool           approval_policy: string        sandbox_mode: string
max_subagents: number       web_search: bool               subagents_enabled: bool
exec_policy: bool           memory_enabled: bool            lsp_enabled: bool
snapshots_enabled: bool     notify_method: string           session_file_mb: number
```

### 2.5 Project Rules & Symbol Index

| Command | Parameters | Return | Description |
|---------|------------|--------|-------------|
| `read_pick_rules` | `workspace_root: String` | `String` | Read `.deepseek/pick-rules.md` (empty if missing) |
| `save_pick_rules` | `workspace_root, content` | `()` | Write project rules file |
| `rebuild_symbol_index` | `workspace: String` | `()` | HTTP `POST /v1/symbol-index/rebuild?workspace=…` (**query**, no JSON body) |
| `get_symbol_index_info` | `workspace: String` | `SymbolIndexInfo` | **IPC only:** read `.deepseek/symbols.json` (status/size/count, stale detection) |
| `delete_symbol_index` | `workspace: String` | `()` | Delete workspace `.deepseek/symbols.json` and related index files |

### 2.6 Sandbox (Windows native + cross-platform overview)

See [SANDBOX_CAPABILITY_MATRIX.md](./SANDBOX_CAPABILITY_MATRIX.md). Desktop Settings → **Sandbox** uses these IPC commands; runtime enforcement is in the sidecar (`crates/runtime-server/src/sandbox/`).

| Command | Parameters | Return | Description |
|---------|------------|--------|-------------|
| `get_windows_sandbox_status` | — | `WindowsSandboxStatus` | Windows-only: setup complete, backend (`elevated` / `unelevated`), configured mode |
| `get_sandbox_platforms_overview` | — | `SandboxPlatformsOverview` | Per-OS sandbox posture for Settings panel |
| `get_sandbox_onboarding_state` | — | `SandboxOnboardingState` | First-run onboarding completion |
| `get_sandbox_settings` | — | `SandboxSettings` | Read `sandbox_mode`, `[windows] sandbox`, private desktop flag |
| `save_sandbox_settings` | `settings: SandboxSettings` | `()` | Write sandbox section → restart sidecar |
| `initialize_windows_sandbox` | `mode: String` | `SandboxSettings` | Windows UAC setup (`elevated` / `unelevated`); writes config markers |

### 2.7 Long-Horizon Tasks (LHT) & Desktop Shell

| Command | Parameters | Return | Description |
|---------|------------|--------|-------------|
| `get_lht_composer_mode` | — | `String` | Composer tri-state: `auto` \| `strict` \| `off` |
| `set_lht_composer_mode` | `mode: String` | `()` | Persist composer LHT mode |
| `get_lht_strict` | — | `bool` | Strict harness gate flag |
| `set_lht_strict` | `strict: bool` | `()` | Persist strict flag |
| `get_lht_settings` | — | `LhtSettings` | Full LHT config block |
| `save_lht_settings` | `settings: LhtSettings` | `()` | Write LHT settings → restart sidecar |
| `apply_lht_preset` | `preset_id: String` | `()` | Apply named LHT preset from `zagens-config` |
| `get_desktop_shell_prefs` | — | `DesktopShellPrefs` | Desktop-only UI prefs (not sidecar system settings) |
| `save_desktop_shell_prefs` | `prefs: DesktopShellPrefs` | `()` | Persist desktop shell prefs |
| `default_composer_workspace` | — | `String` | Default workspace path for new composer sessions |

### 2.8 Lifecycle Hooks

| Command | Parameters | Return | Description |
|---------|------------|--------|-------------|
| `get_hooks_settings` | — | `HooksSettings` | Read `[hooks]` from config (see [desktop/HOOKS.md](../desktop/HOOKS.md)) |
| `save_hooks_settings` | `settings: HooksSettings` | `()` | Write hooks config → restart sidecar |

### 2.9 Embedded Terminal & Multi-Window

| Command | Category |
|---------|----------|
| `spawn_terminal` / `write_terminal` / `resize_terminal` / `kill_terminal` | Embedded PTY (`terminal.rs`) |
| `get_window_label` / `get_window_workspace` / `create_agent_window` / `list_agent_windows` / `focus_agent_window` / `register_window_thread` / `thread_owned_by_window` / `close_current_window` | Multi-window Agent windows (`window_registry.rs`) |

### 2.10 Storage & App Updates

| Command | Parameters | Return | Description |
|---------|------------|--------|-------------|
| `get_storage_pressure` | `workspace_root?: String` | `StoragePressureSnapshot` | Disk guard / workspace size hints for UI warnings |
| `get_update_status` | — | `UpdateStatus` | Tauri updater: available version, download state |
| `install_app_update` | — | `()` | Install downloaded app update and relaunch |

---

## 3. Runtime HTTP API (Channel B)

**Server:** sidecar runs `deepseek-runtime --host 127.0.0.1 --port {port}` (see `sidecar.rs`)
**Listen address:** `http://127.0.0.1:{port}` (default **7878**; use `get_runtime_port`)
**Sidecar env:** `DEEPSEEK_RUNTIME_TOKEN`, `DEEPSEEK_CLIENT_SURFACE=zagens`, optional `DEEPSEEK_API_KEY` (keychain)
**CORS:** sidecar allows `http(s)://tauri.localhost` (WebView cross-origin)
**Serialization:** JSON (request/response bodies)
**Streaming protocol:** SSE (Server-Sent Events) — for `/v1/stream` and `/v1/threads/{id}/events`

### 3.1 Authentication

When `RuntimeApiState.runtime_token` is configured, all `/v1/*` routes are validated by middleware; either:

```http
Authorization: Bearer <runtime_token>
```

or

```http
x-deepseek-runtime-token: <runtime_token>
```

`runtime_token` is UUID v4 from Tauri `main.rs`, passed to sidecar via `DEEPSEEK_RUNTIME_TOKEN`. **WebView does not hold the token** — Tauri production path uses `runtime_proxy.rs` to inject `Authorization: Bearer …` on the Rust side (H06). After `initRuntimeConfig()`, `client.ts` sets `useTauriRuntimeProxy = true`; REST uses `runtime_http`, streaming uses `runtime_post_stream` / `runtime_get_sse`.

Vite frontend-only dev (non-Tauri) may connect directly to sidecar with no Bearer (only when sidecar has no token configured).

If `runtime_token` is **not** configured (some test/dev paths), middleware allows all `/v1/*` — desktop production path always uses a token.

Public endpoints (no auth):
- `GET /health`
- `GET /internal/probe`

### 3.2 General Conventions

- **All request bodies:** `Content-Type: application/json`
- **Retry policy:** WebView retries transient network errors (`TypeError: Failed to fetch`, `NetworkError`) with exponential backoff (5 attempts, base delay 350ms)
- **Runtime readiness probe:** poll `GET /health` on startup (then `GET /v1/sessions` after token ready), up to 90s
- **POST/PATCH with body:** `Content-Type: application/json`; **exception:** `symbol-index/rebuild` uses query only (see §3.3.11)

### 3.2.1 SSE Stable Event Subset v1 (`event:` field)

> **Status: GA · `event_schema_version` = 2**
>
> Both `POST /v1/stream` and `GET /v1/threads/{id}/events` emit SSE.
> Each `RuntimeEventRecord` carries `schema_version: 2`; clients SHOULD ignore
> records where `schema_version` exceeds the known maximum. The table below is the **exhaustive** stable subset;
> implementation: `map_compat_stream_event()` (`crates/runtime-server/src/runtime_api/stream.rs`).

| SSE `event:` | Source | Meaning |
|-------------|--------|---------|
| `turn.started` | `stream_turn` (first frame) | New turn started; includes `thread_id` / `turn_id` / `model` / `mode` / `workspace` |
| `thinking.delta` | `item.delta` (thinking) | Thinking/reasoning delta, `data: { content }` |
| `message.delta` | `item.delta` (agent) | Assistant text delta, `data: { content }` |
| `tool.progress` | `item.delta` (tool) | Tool output stream, `data: { output }` |
| `tool.started` | `item.started` (tool) | Tool call started, `data: { id, name, input }` |
| `tool.completed` | `item.completed` / `item.failed` | Tool call finished, `data: { id, success, output }` |
| `status` | `item.completed` (status) | Status/notification, `data: { message }` |
| `error` | `item.completed` (error) | Error message, `data: { message }` |
| `approval.required` | `approval.required` | User approval required for tool execution, `data: { id, tool_name, description, … }` |
| `sandbox.denied` | `sandbox.denied` | Sandbox policy denied |
| `turn.completed` | `turn.completed` | Turn finished, `data: { usage, turn_summary? }` |
| `agent.spawned` | `agent.spawned` | Sub-agent started, `data: { agent_id, prompt? }` |
| `agent.progress` | `agent.progress` | Sub-agent status update, `data: { agent_id, status }` |
| `agent.completed` | `agent.completed` | Sub-agent finished, `data: { agent_id, result }` |
| `agent.list` | `agent.list` | Sub-agent list, `data: { agents }` |
| `craft.verdict` | `craft.verdict` | CRAFT structured verdict, `data: { agent_id, agent_type, task_id?, verdict, summary?, items }` |
| `craft.board_updated` | `craft.board_updated` | CRAFT blackboard partition update, `data: { task_id, partition, agent_id }` |
| `panel.checklist` | `panel.checklist` | Checklist panel data |
| `panel.scratchpad` | `panel.scratchpad` | Scratchpad panel data |
| `panel.context` | `panel.context` | Context panel data |
| `done` | `stream_turn` (last frame) | Stream end, no data |

**`turn_summary` sub-object** (only on `turn.completed`, optional):
```json
{
  "step_count": 3,
  "tool_names": ["read_file", "grep_files"],
  "end_reason": null
}
```

**Client guidance:**
- Unknown event names SHOULD be ignored (forward compatibility).
- When `schema_version` increments, new fields are additive; clients MUST NOT fail on unknown fields.
- WebView normalizes to UI events in `streamNormalize.ts`.

---

### 3.3 Endpoint Reference

#### 3.3.1 Health Checks

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/health` | No | Runtime liveness check |

**`GET /health` response:**

```json
{
  "status": "ok",
  "service": "zagens-runtime-api",
  "mode": "local",
  "event_schema_version": 2
}
```

| GET | `/internal/probe` | No | Internal readiness / troubleshooting (PID, start time, token fingerprint, sidecar version) |

**`GET /internal/probe` response** (summary):

```json
{
  "status": "ok",
  "pid": 12345,
  "started_at_ms": 1710000000000,
  "token_fingerprint": "...",
  "version": "0.7.5"
}
```

(`version` is **deepseek-runtime / runtime crate** version, not Zagens shell version.)

#### 3.3.2 Streaming Chat

| Method | Path | Description |
|--------|------|-------------|
| POST | `/v1/stream` | **Core streaming endpoint** — create anonymous thread and start one turn; SSE stream |

**Request body** (`StreamTurnRequest`):

```json
{
  "prompt": "User input text",
  "workspace": "/path/to/project",
  "mode": "agent",
  "model": "deepseek-v4-pro",
  "auto_approve": false,
  "trust_mode": false,
  "allow_shell": false,
  "route_intent": "coding"
}
```

**SSE event stream:** Each event is `event:` + `data:` (mostly JSON) blocks separated by `\n\n`. Event names: §3.2.1.

**Two chat paths:**

| Path | Use case |
|------|----------|
| `POST /v1/stream` | Quick single turn (anonymous thread + one turn) |
| `POST /v1/threads` + `…/turns` + `GET …/events` | Persistent thread, multi-turn, approval/interrupt |

#### 3.3.3 Sessions

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/sessions` | List all sessions |
| GET | `/v1/sessions/{id}` | Session detail (messages, system prompt) |
| DELETE | `/v1/sessions/{id}` | Delete session (200 or 204) |
| POST | `/v1/sessions/{id}/resume-thread` | Resume archived session as runtime thread |

**`GET /v1/sessions/{id}` response** (`SessionDetail`):

```json
{
  "metadata": { "id": "...", "name": "...", "title": "...", "created_at": 0, "updated_at": 0 },
  "messages": [
    { "role": "user", "content": [{ "type": "text", "text": "..." }] }
  ],
  "system_prompt": "..."
}
```

#### 3.3.4 Threads — Core Conversation Unit

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/threads` | List all threads |
| POST | `/v1/threads` | Create new thread |
| GET | `/v1/threads/summary` | Thread summary list |
| GET | `/v1/threads/{id}` | Thread detail + `latest_seq` |
| PATCH | `/v1/threads/{id}` | Update thread properties |
| POST | `/v1/threads/{id}/resume` | Resume thread |
| POST | `/v1/threads/{id}/fork` | Fork thread |
| POST | `/v1/threads/{id}/fork-at-user-message` | Fork at a specific user message |
| POST | `/v1/threads/{id}/edit-last-turn` | Edit and replay from last user turn |
| GET | `/v1/threads/{id}/context` | Thread context summary (tokens, compaction hints) |
| GET | `/v1/threads/{id}/harness/task-graph` | LHT harness task graph snapshot |
| GET | `/v1/threads/{id}/harness/cycles` | LHT harness cycle status |
| GET | `/v1/threads/{id}/scratchpad/status` | Scratchpad agent status |
| POST | `/v1/threads/{id}/scratchpad/init` | Initialize scratchpad for thread |
| POST | `/v1/threads/{id}/compact` | Compact thread context |
| POST | `/v1/threads/{id}/turns` | Start a turn on this thread |
| POST | `/v1/threads/{id}/turns/{turn_id}/steer` | Steer conversation direction |
| POST | `/v1/threads/{id}/turns/{turn_id}/resolve-approval` | Handle tool approval (approve/deny) |
| POST | `/v1/threads/{id}/turns/{turn_id}/interrupt` | Interrupt running turn |
| GET | `/v1/threads/{id}/checklist` | Thread-associated checklist |
| GET | `/v1/threads/{id}/events` | Subscribe to thread event stream (SSE) |
| POST | `/v1/threads/{id}/persist-session` | Persist thread as archivable session |
| GET | `/v1/threads/{id}/snapshots` | List thread git snapshots (`?limit=N`) |
| POST | `/v1/threads/{id}/snapshots/restore` | Restore to snapshot (`{ "n": N }`) |

**`PATCH /v1/threads/{id}` request body** (`PatchThreadBody`):

```json
{
  "archived": false,
  "allow_shell": true,
  "trust_mode": false,
  "auto_approve": true,
  "model": "deepseek-v4-flash",
  "mode": "agent",
  "title": "Refactor task",
  "system_prompt": "You are a Rust expert",
  "workspace": "/path/to/workspace"
}
```

**`POST /v1/threads/{id}/turns` request body:**

```json
{
  "prompt": "Please fix clippy warnings",
  "model": "deepseek-v4-pro",
  "mode": "agent",
  "allow_shell": true,
  "trust_mode": false,
  "auto_approve": false,
  "route_intent": "coding"
}
```

**`POST /v1/threads/{id}/turns/{turn_id}/resolve-approval` request body:**

```json
{
  "tool_call_id": "...",
  "decision": "approve"
}
```

##### Thread Workspace Browse

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/threads/{id}/workspace/browse` | List thread workspace directory (`?path=`) |
| GET | `/v1/threads/{id}/workspace/file` | Read thread workspace file (`?path=`) |

##### Composer Workspace Browse (no thread context)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/workspace/browse` | List any workspace directory (`?workspace=&path=`) |
| GET | `/v1/workspace/file` | Read any workspace file (`?workspace=&path=`) |
| GET | `/v1/workspace/status` | Workspace status |
| GET | `/v1/office/environment` | Office tooling environment probe (Python, bundled scripts) |

**`GET ../workspace/browse` response:**

```json
{
  "workspace": "/path/to/root",
  "path": "src",
  "entries": [
    { "name": "main.rs", "kind": "file", "size": 12400 },
    { "name": "components", "kind": "dir" }
  ]
}
```

**`GET ../workspace/file` response:**

```json
{
  "path": "src/main.rs",
  "content": "fn main() { ... }",
  "truncated": false,
  "language_hint": "rust"
}
```

#### 3.3.5 Tasks — Durable Background Work

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/tasks` | List all tasks |
| POST | `/v1/tasks` | Create task |
| POST | `/v1/tasks/clear` | Clear completed/cancelled tasks |
| GET | `/v1/tasks/{id}` | Task detail |
| POST | `/v1/tasks/{id}/cancel` | Cancel task |
| GET | `/v1/resume-tasks/{thread_id}` | Resume task for thread |

#### 3.3.6 Skills

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/skills` | List all skills |
| POST | `/v1/skills` | Create skill |
| POST | `/v1/skills/import` | Import skill from local path/archive |
| POST | `/v1/skills/install` | Install skill from remote URL/registry |

#### 3.3.6a Blackboards & Topic Memory

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/blackboards` | List CRAFT blackboards |
| GET | `/v1/blackboards/{id}` | Blackboard detail |
| GET | `/v1/topic-memory` | B2 topic memory snapshot |

#### 3.3.7 Automations

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/automations` | List automation rules |
| POST | `/v1/automations` | Create automation rule |
| GET | `/v1/automations/{id}` | Get automation |
| PATCH | `/v1/automations/{id}` | Update automation |
| DELETE | `/v1/automations/{id}` | Delete automation |
| POST | `/v1/automations/{id}/run` | Run automation |
| POST | `/v1/automations/{id}/pause` | Pause automation |
| POST | `/v1/automations/{id}/resume` | Resume automation |
| GET | `/v1/automations/{id}/runs` | List automation run history |

#### 3.3.8 MCP (Model Context Protocol) Servers

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/apps/mcp/servers` | List MCP servers |
| POST | `/v1/apps/mcp/servers` | Add MCP server |
| GET | `/v1/apps/mcp/servers/{name}` | Get MCP server config |
| PUT | `/v1/apps/mcp/servers/{name}` | Update MCP server config |
| DELETE | `/v1/apps/mcp/servers/{name}` | Delete MCP server |
| GET | `/v1/apps/mcp/tools` | List MCP tools (`?server=`) |
| POST | `/v1/apps/mcp/config/merge` | Merge MCP config JSON into `mcp.json` |
| POST | `/v1/apps/mcp/reload` | Reload MCP pool from disk config |
| GET | `/v1/apps/mcp/discover` | Discover MCP servers (stdio/SSE probe) |
| GET | `/v1/apps/mcp/calls` | List recent MCP tool call records |

**`POST /v1/apps/mcp/servers` request body:**

```json
{
  "name": "my-server",
  "command": "npx",
  "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
  "url": null
}
```

#### 3.3.9 Routing Rules

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/apps/routing/rules` | Get model routing rules |
| PUT | `/v1/apps/routing/rules` | Set routing rules; body `{ "rules": [ ... ] }` |

#### 3.3.10 Usage

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/usage` | Usage statistics |
| `?since=&until=&group_by=` | | Aggregate by model/date |

#### 3.3.11 Symbol Index

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| POST | `/v1/symbol-index/rebuild?workspace=/abs/path` | Bearer | Rebuild index; **query param only** `workspace`, no request body |

Success response example (JSON):

```json
{
  "status": "ok",
  "path": "/path/to/.deepseek/symbols.json",
  "symbol_count": 42
}
```

Index **status/size/stale** via IPC `get_symbol_index_info`; no corresponding GET HTTP route.

---

## 4. Authentication Flow (H06)

```
┌──────────┐          ┌──────────┐          ┌──────────────┐
│  Tauri   │          │  WebView │          │   Sidecar    │
│  main.rs │          │   JS     │          │deepseek-runtime│
└────┬─────┘          └────┬─────┘          └──────┬───────┘
     │                     │                       │
     │ ① uuid v4 token     │                       │
     │  (AppState)         │                       │
     │ ② spawn sidecar     │                       │
     │  DEEPSEEK_RUNTIME_  │                       │
     │  TOKEN=zagens       │                       │
     │────────────────────────────────────────────►│
     │                     │                       │
     │                     │ ③ invoke runtime_http │
     │                     │   / runtime_post_     │
     │                     │   stream / runtime_   │
     │                     │   get_sse             │
     │                     │──────────────────────►│
     │  Rust injects Bearer│                       │
     │────────────────────────────────────────────►│
     │                     │      200 / SSE ◄──────│
```

1. Tauri `main.rs` generates `uuid v4` token, stores in `AppState`
2. Sidecar starts with `DEEPSEEK_RUNTIME_TOKEN` env var
3. WebView calls `runtime_http` / `runtime_post_stream` / `runtime_get_sse` (**does not** obtain token)
4. `runtime_proxy.rs` injects `Authorization: Bearer <token>` on each request

**Token security:** Bearer **does not** enter WebView or **localStorage** / `window`; see CHANGELOG 2026-05-20 H06 follow-up.

---

## 5. Error Handling

### 5.1 Runtime HTTP Error Format

Failed responses are often **plain text** or short JSON (handler-dependent). WebView **does not** assume a fixed schema; uniformly `await res.text()` then construct `Error`.

WebView wraps as `Error` with `status` property:

```typescript
// Unified pattern in client.ts
if (!res.ok) {
  const text = await res.text();
  const err = new Error(`HTTP ${res.status}: ${text}`);
  (err as Error & { status?: number }).status = res.status;
  throw err;
}
```

### 5.2 Transient Error Retry

`isTransientRuntimeFetchError()` detects:
- `TypeError` (fetch failure, sidecar not ready)
- `Error` message containing `Failed to fetch` / `NetworkError` / `Load failed`

On transient errors, auto-retry 5 times (exponential backoff, base delay 350ms).

### 5.3 Tauri IPC Errors

Returns `Result<T, String>`; Rust errors passed as strings, caught as `Error` on frontend.

---

## 6. Sidecar Lifecycle

```
┌───────────────┐
│  Tauri start  │
└───────┬───────┘
        ▼
┌───────────────────────────────────────┐
│  sidecar.rs: start_and_monitor()       │
│                                        │
│  spawn deepseek-runtime --http         │
│  + health check polling                │
│  + crash auto-restart (backoff)        │
│  + API Key change → restart notify     │
│  + logs to sidecar.log /               │
│    supervisor.log                      │
└───────────────────────────────────────┘
```

Key parameters:
- **Startup probe:** 60 retries, first 60ms, then 200ms, ~12s total
- **Health heartbeat:** 5s interval
- **Crash restart:** ≥3 rapid crashes within 60s → pause auto-restart, notify user
- **Connection refused threshold:** 2 (treat port as dead immediately)
- **Busy timeout threshold:** 12 (process alive but overloaded, grace wait)
- **API Key change:** `restart_sidecar` triggers restart, 450ms debounce

---

## 7. Data Flow Diagrams

### 7.1 Starting a Conversation

```
WebView                          Sidecar                    LLM API
  │                                │                          │
  │ POST /v1/stream { prompt, ... }│                          │
  │───────────────────────────────►│                          │
  │                                │ POST /chat/completions   │
  │                                │─────────────────────────►│
  │                                │      SSE streaming ◄────│
  │◄── SSE: thinking.delta ───────│                          │
  │◄── SSE: tool.started ─────────│                          │
  │◄── SSE: message.delta ────────│                          │
  │◄── SSE: done ─────────────────│                          │
```

### 7.2 Tool Approval

```
WebView                          Sidecar
  │                                │
  │◄── SSE: approval.required ────│  (user approval required)
  │                                │
  │ POST …/resolve-approval        │
  │ { tool_call_id, decision }     │
  │───────────────────────────────►│
  │                                │  continue tool execution
  │◄── SSE: tool.completed ───────│
```

---

## 8. Endpoint Summary

### Channel A: Tauri IPC

> Full list: `crates/desktop/src/main.rs` `generate_handler!` (**64** commands as of 2026-06-12). **Excludes** removed `get_runtime_token`. Grouped by category (order differs from registration):

| Category | Commands |
|----------|----------|
| Runtime proxy | `get_runtime_port`, `runtime_http`, `runtime_post_stream`, `runtime_get_sse`, `runtime_cancel_sse` |
| Platform / locale | `get_platform_info`, `get_os_theme`, `get_locale`, `set_app_locale`, `restart_sidecar` |
| Sandbox | `get_windows_sandbox_status`, `get_sandbox_platforms_overview`, `get_sandbox_onboarding_state`, `get_sandbox_settings`, `save_sandbox_settings`, `initialize_windows_sandbox` |
| API key / vision | `get_api_key_status`, `save_deepseek_api_key`, `clear_deepseek_api_key`, `get_vision_bridge_status`, `save_vision_bridge`, `clear_vision_bridge`, `vision_transcribe_image` |
| File / export | `read_thread_workspace_binary`, `read_workspace_binary_at_root`, `open_in_shell`, `open_with_system_app`, `open_external_url`, `export_thread_json`, `export_session_json` |
| System / LHT / hooks | `get_system_settings`, `save_system_settings`, `get_lht_*`, `set_lht_*`, `apply_lht_preset`, `get_hooks_settings`, `save_hooks_settings`, `get_desktop_shell_prefs`, `save_desktop_shell_prefs`, `default_composer_workspace` |
| Rules / symbol index | `read_pick_rules`, `save_pick_rules`, `rebuild_symbol_index`, `get_symbol_index_info`, `delete_symbol_index` |
| Storage | `get_storage_pressure` |
| Terminal | `spawn_terminal`, `write_terminal`, `resize_terminal`, `kill_terminal` |
| Multi-window | `get_window_label`, `get_window_workspace`, `create_agent_window`, `list_agent_windows`, `focus_agent_window`, `register_window_thread`, `thread_owned_by_window`, `close_current_window` |
| App updater | `get_update_status`, `install_app_update` |

### Channel B: Runtime HTTP

| # | Method | Path | Category |
|---|--------|------|----------|
| 1 | GET | `/health` | Health |
| 2 | GET | `/internal/probe` | Health |
| 3 | POST | `/v1/stream` | Streaming chat |
| 4 | GET | `/v1/sessions` | Sessions |
| 5 | GET | `/v1/sessions/{id}` | Sessions |
| 6 | DELETE | `/v1/sessions/{id}` | Sessions |
| 7 | POST | `/v1/sessions/{id}/resume-thread` | Sessions |
| 8 | GET | `/v1/threads` | Threads |
| 9 | POST | `/v1/threads` | Threads |
| 10 | GET | `/v1/threads/summary` | Threads |
| 11 | GET | `/v1/threads/{id}` | Threads |
| 12 | PATCH | `/v1/threads/{id}` | Threads |
| 13 | POST | `/v1/threads/{id}/resume` | Threads |
| 14 | POST | `/v1/threads/{id}/fork` | Threads |
| 15 | POST | `/v1/threads/{id}/fork-at-user-message` | Threads |
| 16 | POST | `/v1/threads/{id}/edit-last-turn` | Threads |
| 17 | GET | `/v1/threads/{id}/context` | Threads |
| 18 | GET | `/v1/threads/{id}/harness/task-graph` | Threads (LHT) |
| 19 | GET | `/v1/threads/{id}/harness/cycles` | Threads (LHT) |
| 20 | GET | `/v1/threads/{id}/scratchpad/status` | Threads (scratchpad) |
| 21 | POST | `/v1/threads/{id}/scratchpad/init` | Threads (scratchpad) |
| 22 | POST | `/v1/threads/{id}/compact` | Threads |
| 23 | POST | `/v1/threads/{id}/turns` | Threads |
| 24 | POST | `/v1/threads/{id}/turns/{turn_id}/steer` | Threads |
| 25 | POST | `/v1/threads/{id}/turns/{turn_id}/resolve-approval` | Threads |
| 26 | POST | `/v1/threads/{id}/turns/{turn_id}/interrupt` | Threads |
| 27 | GET | `/v1/threads/{id}/checklist` | Threads |
| 28 | GET | `/v1/threads/{id}/events` | Threads (SSE) |
| 29 | POST | `/v1/threads/{id}/persist-session` | Threads |
| 30 | GET | `/v1/threads/{id}/snapshots` | Threads |
| 31 | POST | `/v1/threads/{id}/snapshots/restore` | Threads |
| 32 | GET | `/v1/threads/{id}/workspace/browse` | Workspace browse |
| 33 | GET | `/v1/threads/{id}/workspace/file` | Workspace browse |
| 34 | GET | `/v1/workspace/browse` | Workspace browse |
| 35 | GET | `/v1/workspace/file` | Workspace browse |
| 36 | GET | `/v1/workspace/status` | Workspace browse |
| 37 | GET | `/v1/office/environment` | Office environment |
| 38 | GET | `/v1/tasks` | Tasks |
| 39 | POST | `/v1/tasks` | Tasks |
| 40 | POST | `/v1/tasks/clear` | Tasks |
| 41 | GET | `/v1/tasks/{id}` | Tasks |
| 42 | POST | `/v1/tasks/{id}/cancel` | Tasks |
| 43 | GET | `/v1/resume-tasks/{thread_id}` | Tasks |
| 44 | GET | `/v1/blackboards` | Blackboards |
| 45 | GET | `/v1/blackboards/{id}` | Blackboards |
| 46 | GET | `/v1/topic-memory` | Topic memory |
| 47 | GET | `/v1/skills` | Skills |
| 48 | POST | `/v1/skills` | Skills |
| 49 | POST | `/v1/skills/import` | Skills |
| 50 | POST | `/v1/skills/install` | Skills |
| 51 | GET | `/v1/automations` | Automations |
| 52 | POST | `/v1/automations` | Automations |
| 53 | GET | `/v1/automations/{id}` | Automations |
| 54 | PATCH | `/v1/automations/{id}` | Automations |
| 55 | DELETE | `/v1/automations/{id}` | Automations |
| 56 | POST | `/v1/automations/{id}/run` | Automations |
| 57 | POST | `/v1/automations/{id}/pause` | Automations |
| 58 | POST | `/v1/automations/{id}/resume` | Automations |
| 59 | GET | `/v1/automations/{id}/runs` | Automations |
| 60 | GET | `/v1/apps/mcp/servers` | MCP |
| 61 | POST | `/v1/apps/mcp/servers` | MCP |
| 62 | GET | `/v1/apps/mcp/servers/{name}` | MCP |
| 63 | PUT | `/v1/apps/mcp/servers/{name}` | MCP |
| 64 | DELETE | `/v1/apps/mcp/servers/{name}` | MCP |
| 65 | GET | `/v1/apps/mcp/tools` | MCP |
| 66 | POST | `/v1/apps/mcp/config/merge` | MCP |
| 67 | POST | `/v1/apps/mcp/reload` | MCP |
| 68 | GET | `/v1/apps/mcp/discover` | MCP |
| 69 | GET | `/v1/apps/mcp/calls` | MCP |
| 70 | GET | `/v1/apps/routing/rules` | Routing |
| 71 | PUT | `/v1/apps/routing/rules` | Routing |
| 72 | GET | `/v1/usage` | Usage |
| 73 | POST | `/v1/symbol-index/rebuild` | Symbol index |

---

## 9. Source File Index

| Component | File | Description |
|-----------|------|-------------|
| Tauri command registration | `crates/desktop/src/main.rs` | `generate_handler!` — **64** IPC commands |
| Tauri command impl | `crates/desktop/src/commands.rs` | Keys, settings, binary preview, symbol index IPC |
| Runtime HTTP proxy | `crates/desktop/src/runtime_proxy.rs` | H06 — Bearer injection, SSE forwarding |
| Sidecar process mgmt | `crates/desktop/src/sidecar.rs` | spawn `deepseek-runtime`, health check, restart |
| WebView HTTP client | `crates/desktop/web-ui/src/api/client.ts` | `initRuntimeConfig`, proxy branch, readiness probe |
| SSE normalization | `crates/desktop/web-ui/src/api/streamNormalize.ts` | wire `event` → UI events |
| Runtime HTTP routes | `crates/runtime-server/src/runtime_api/router.rs` | `build_router` |
| Protocol types | `crates/protocol/src/` | Shared DTOs (if any) |
| Agent prompt (sidecar) | `crates/runtime-server/src/prompts/base.md` etc. | Includes hallucination-control sub-rules (Capability / Architecture Claims) |

> `crates/app-server/` is for other monorepo entry points; **Zagens desktop path does not use this crate**. Line counts are approximate at time of writing; verify in repo.

---

## 10. Related Documentation

| Document | Content |
|----------|---------|
| [RUNTIME_ARCHITECTURE.md](./RUNTIME_ARCHITECTURE.md) | Three-layer model, crate deps, dual persistence diagrams |
| [SANDBOX_CAPABILITY_MATRIX.md](./SANDBOX_CAPABILITY_MATRIX.md) | Sandbox backends (incl. Windows native) |
| [TOOLS_PRINCIPLES.md](./TOOLS_PRINCIPLES.md) | Tool system architecture and execution flow |
| Maintainer: `doc_Private/docs/tech/RUNTIME_EVOLUTION_ROADMAP.md` | Evolution roadmap §3 current snapshot |
| `crates/runtime-server/src/prompts/base.md` | Hallucination-control sub-rules (Capability / Architecture claims) |
| Regression tests (maintainer) | `doc_Private/docs/tui/REGRESSION_TESTS.md` — hallucination control and parallel scheduling regression cases |
| [craft-v2-improvements.md](../craft-v2-improvements.md) · [TOOLS_PRINCIPLES.md](./TOOLS_PRINCIPLES.md) §3.7 | CRAFT improvements; sub-agent write path |

---

## Revision History

| Date | Notes |
|------|-------|
| 2026-06-12 | Align with **0.7.5**: 64 IPC (sandbox, LHT, hooks, updater), 73 HTTP routes (harness, scratchpad, blackboards, topic-memory, MCP reload/discover/calls, office environment) |
| 2026-05-26 | D6 Phase B: sidecar path → `crates/runtime-server`; remove `deepseek-tui` / CLI production narrative |
| 2026-05-25 | Align with code: Zagens 0.4.3, `runtime_api/router.rs`, H06 proxy auth (remove `get_runtime_token`), IPC ~41, `DEEPSEEK_CLIENT_SURFACE=zagens` |
| 2026-05-18 | Code review fixes: health/probe, BinaryFileResponse, SSE event names, symbol-index query, auth header, 25 IPC / 56 HTTP, app-server boundary |
| 2026-05-18 | Initial version (Zagens dual-channel API) |
