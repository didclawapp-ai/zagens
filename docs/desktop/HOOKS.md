# Zagens desktop — lifecycle hooks

> **Status (2026-06):** Config-driven shell hooks are wired through the runtime engine turn loop. This document covers **user-facing hook configuration** — not the separate `zagens-hooks` crate used for SSE audit sinks.

## Overview

Hooks run **user-defined shell commands** at agent lifecycle points: session boundaries, message submit, tool calls, compaction, sub-agent spawn, errors, and shell environment injection.

| Surface | Location |
|---------|----------|
| Global config | `~/.zagens/config.toml` → `[hooks]` / `[[hooks.hooks]]` |
| Desktop UI | Settings → **Hooks** (`HooksPanel.tsx`) — edits global config; restarts sidecar on save |
| Project hooks | `{workspace}/.zagens/hooks.toml`, `.zagens/hooks.json`, `.cursor/hooks.json` |
| Engine executor | [`hooks.rs`](../../crates/runtime-server/src/hooks.rs), [`hook_dispatch.rs`](../../crates/runtime-server/src/core/engine/hook_dispatch.rs) |
| Load / merge / Cursor aliases | [`hooks_load.rs`](../../crates/runtime-server/src/hooks_load.rs) |
| Example scripts + smoke test | [`scripts/hooks/`](../../scripts/hooks/) |

**Merge order:** global `config.toml` hooks → project TOML → project JSON → `.cursor/hooks.json`. Project hooks append after global entries. Project `enabled = false` disables only local entries.

## Events

| Event | Blocking? | When it fires |
|-------|-----------|---------------|
| `session_start` | No | First turn of a session |
| `session_end` | No | Session quit / shutdown |
| `message_submit` | **Yes** | Before user message is sent to the LLM |
| `tool_call_before` | **Yes** | Before a tool executes |
| `tool_call_after` | No | After a tool completes (success or failure) |
| `mode_change` | No | Plan / Agent / YOLO switch |
| `on_error` | No | Engine error path |
| `shell_env` | No (fail-open) | Immediately before each `exec_shell`; stdout merged as env vars |
| `pre_compact` | No | Before context compaction (manual or auto) |
| `post_compact` | No | After successful compaction |
| `subagent_start` | **Yes** | Before a sub-agent task is spawned |
| `subagent_end` | No | When a sub-agent reaches a terminal state |
| `night_queue_enqueue` | No | After a task is appended to `.zagens/night_queue.json` |
| `night_queue_run_start` | No | Before a batch `queue run` starts (CLI, HTTP, or scheduled) |
| `night_queue_run_end` | No | After a batch `queue run` finishes (includes `DEEPSEEK_NIGHT_QUEUE_*` env vars) |

### Cursor-compatible aliases

Event names in config or JSON may use Cursor-style aliases. Examples:

| Alias | Maps to |
|-------|---------|
| `beforeShell`, `beforeShellExecution` | `tool_call_before` + implicit `exec_shell` filter |
| `afterShell`, `afterShellExecution` | `tool_call_after` + implicit `exec_shell` filter |
| `beforeFileEdit` / `afterFileEdit` | tool before/after + `file_write` category |
| `preToolUse` / `postToolUse` | `tool_call_before` / `tool_call_after` |
| `beforeSubmitPrompt` | `message_submit` |
| `subagentStart` / `subagentStop` | `subagent_start` / `subagent_end` |
| `stop` | `session_end` |

Full alias table: `parse_hook_event_name()` in [`hooks_load.rs`](../../crates/runtime-server/src/hooks_load.rs).

## Hook protocol

### Input (stdin)

Every synchronous hook receives JSON on stdin:

```json
{
  "event": "tool_call_before",
  "context": {
    "tool_name": "exec_shell",
    "tool_args": "{\"command\":\"echo hi\"}",
    "mode": "agent",
    "session_id": "sess_abc123",
    "workspace": "/path/to/project",
    "model": "deepseek-v4-flash",
    "subagent_id": null,
    "compaction_manual": null
  }
}
```

Parallel **environment variables** (`DEEPSEEK_*`) are also set — useful for shell scripts that do not parse JSON. See `HookContext::to_env_vars()` in [`hooks.rs`](../../crates/runtime-server/src/hooks.rs).

### Output (stdout) — blocking events

For `message_submit`, `tool_call_before`, and `subagent_start`:

| Mechanism | Effect |
|-----------|--------|
| Exit code **2** | Block the action |
| stdout `{"decision":"deny"}` or `{"allow":false}` | Block (optional `reason` / `message`) |
| stdout `{"updatedInput":{…}}` / `updated_input` / `toolInput` | **Rewrite tool args** (`tool_call_before` only) |

Non-blocking events ignore deny semantics unless `continue_on_error = false` on the hook definition.

### `shell_env` hooks

Stdout must be `KEY=VALUE` lines (one per line). Parsed keys are merged into the **next** `exec_shell` environment. Later hooks override earlier ones. Hook failure does **not** abort the shell call. Key names (never values) are also written to the sensitive audit log.

## Conditions

Hooks may run unconditionally or with a filter:

| Condition type | Example |
|----------------|---------|
| `always` | Default — always run |
| `tool_name` | Only `exec_shell` |
| `tool_name_regex` | Rust regex on tool name (also matches `subagent_type` when set) |
| `tool_category` | `shell`, `file_write`, `safe`, … |
| `mode` | `plan`, `agent`, `yolo` (case-insensitive) |
| `exit_code` | For `tool_call_after` |
| `all` / `any` | Nested AND / OR combinations |

Configure in TOML:

```toml
[[hooks.hooks]]
event = "tool_call_before"
command = "scripts/hooks/deny_tool.sh"
condition = { type = "tool_name", name = "exec_shell" }

[[hooks.hooks]]
event = "tool_call_before"
command = "scripts/hooks/gate_risky.sh"
condition = { type = "all", conditions = [
  { type = "tool_name_regex", pattern = "exec_.*" },
  { type = "mode", mode = "agent" },
]}
```

The desktop Hooks panel supports all condition types including nested **all / any** editors.

## Configuration

### Global (`~/.zagens/config.toml`)

```toml
[hooks]
enabled = true
default_timeout_secs = 30
# working_dir = "/optional/override"
audit_jsonl = "~/.zagens/hooks-audit.jsonl"

[[hooks.hooks]]
event = "session_start"
command = "echo 'session started'"

[[hooks.hooks]]
name = "block-secrets"
event = "message_submit"
command = "scripts/hooks/deny_message.sh"
timeout_secs = 10
continue_on_error = false
```

| Field | Description |
|-------|-------------|
| `enabled` | Master switch |
| `default_timeout_secs` | Override per-hook timeout when hook omits `timeout_secs` |
| `working_dir` | CWD for hook processes (default: session workspace) |
| `audit_jsonl` | Optional JSONL audit log (one JSON object per hook execution) |
| `background` | Fire-and-forget (do not wait) |
| `continue_on_error` | When `false`, stop remaining hooks in the chain on failure |

Saving from the desktop Hooks panel writes this table and **restarts the sidecar**.

### Project-level

| File | Format |
|------|--------|
| `.zagens/hooks.toml` | Same schema as `[[hooks.hooks]]` tables |
| `.zagens/hooks.json` | Cursor-style `{ "version": 1, "hooks": { "sessionStart": [...] } }` |
| `.cursor/hooks.json` | Same as above — loaded for Cursor migration |

Cursor JSON entries support `command`, `matcher`, `timeout`, `failClosed`, and `name`. **Prompt-type** hooks (`"type": "prompt"`) are skipped with a warning (command hooks only). `failClosed: true` maps to `continue_on_error = false`.

Example: [`scripts/hooks/examples/hooks.cursor.json`](../../scripts/hooks/examples/hooks.cursor.json).

## Sub-agent and compaction hooks

- **`subagent_start`** runs in the spawn path before the sub-agent task is scheduled. A deny blocks spawn and surfaces an error to the parent agent.
- **`subagent_end`** runs when the sub-agent task finishes (any terminal status). Context includes `subagent_id`, `subagent_type`, `subagent_status`, and a short summary in `message`.
- **`pre_compact`** / **`post_compact`** wrap context compaction. `post_compact` context includes `compaction_manual`, `compaction_messages_before`, and `compaction_messages_after` when available.

## Example scripts and smoke test

Repository examples under [`scripts/hooks/`](../../scripts/hooks/):

| Script | Demonstrates |
|--------|--------------|
| `echo_context.sh` | Read stdin JSON, log to stderr |
| `deny_message.sh` | Block when message contains `BLOCK_ME` (exit 2) |
| `deny_tool.sh` | stdout JSON deny |
| `updated_input.sh` | stdout `updatedInput` rewrite |
| `shell_env.sh` | `KEY=VALUE` lines for `shell_env` |
| `subagent_echo.sh` | Sub-agent lifecycle logging |

Run unit tests + script smoke checks:

```powershell
powershell -File scripts/hooks/run_smoke.ps1
```

```bash
sh scripts/hooks/run_smoke.sh
```

## Distinction: config hooks vs `zagens-hooks` crate

| System | Purpose |
|--------|---------|
| **`[hooks]` in config.toml** | User shell commands at lifecycle points (this document) |
| **`crates/hooks` (`zagens-hooks`)** | Pluggable `HookSink` for SSE/streaming audit (JSONL, webhook, stdout) — separate event model, not loaded from `config.toml` |

When extending hook behavior for end users, prefer the config hook pipeline unless you are wiring runtime telemetry sinks.

## Related docs

- [`config.example.toml`](../../config.example.toml) — commented hook templates
- [RUNTIME_ARCHITECTURE.md](../tech/RUNTIME_ARCHITECTURE.md)
- [SANDBOX_CAPABILITY_MATRIX.md](../tech/SANDBOX_CAPABILITY_MATRIX.md) — shell and tool boundaries hooks often gate
