# Zagens Tool System Principles

> **Document path:** `docs/tech/TOOLS_PRINCIPLES.md` (same `docs/tech/` tree as [API_DESIGN.md](./API_DESIGN.md) and [RUNTIME_ARCHITECTURE.md](./RUNTIME_ARCHITECTURE.md))  
> **Zagens shell version:** 0.8.8+ | **Last updated:** 2026-07-21 | **Authoritative implementation:** `crates/runtime-server/src/tools/` (formerly `crates/tui/src/tools/`), `registry.rs`  
> **Roadmap status:** see [§9 Innovation roadmap status](#9-innovation-roadmap-status-2026-07)

---

## 1. Architecture Overview

The Zagens tool system uses a **unified abstraction + layered registration** architecture. Every tool implements the same `ToolSpec` trait, registers into `ToolRegistry`, and is scheduled by the engine in the Agent loop.

```
┌─────────────────────────────────────────────────────────┐
│                    Agent Engine (core)                   │
│                                                         │
│  ┌──────────┐   ┌──────────────┐   ┌────────────────┐  │
│  │ LLM      │──▶│ ToolRegistry │──▶│ ToolSpec::     │  │
│  │ tool_call│   │ .execute()   │   │   execute()    │  │
│  └──────────┘   └──────────────┘   └───────┬────────┘  │
│                                             │           │
│  ┌──────────────────────────────────────────┘           │
│  │  ToolContext { workspace, sandbox, lsp, … }          │
│  │  ToolResult { content, success, metadata }           │
│  └──────────────────────────────────────────────────────┤
│                         │                               │
│  ┌──────────────────────▼─────────────────────────────┐ │
│  │  LargeOutputRouter (large results via V4-Flash)    │ │
│  │  LSP Diagnostics (auto-inject after edits)         │ │
│  └────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

### 1.1 Core Trait: `ToolSpec`

Defined in `crates/runtime-server/src/tools/spec.rs` (public types from `crates/tools/src/lib.rs`). Each tool implements:

| Method | Purpose |
|--------|---------|
| `name()` | Unique tool identifier (e.g. `"read_file"`) |
| `description()` | Tool description sent to LLM |
| `input_schema()` | JSON Schema for accepted parameters |
| `capabilities()` | Capability tags: `ReadOnly`, `WritesFiles`, `ExecutesCode`, `Network`, `Sandboxable`, `RequiresApproval` |
| `approval_requirement()` | Approval level: `Auto`, `Suggest`, `Required` |
| `supports_parallel()` | Whether parallel execution with other tools is allowed |
| `defer_loading()` | Whether exposure to LLM is deferred (MCP tools default deferred) |
| `execute(input, context)` | Core execution logic |

### 1.2 Execution Context: `ToolContext`

```rust
pub struct ToolContext {
    pub workspace: PathBuf,             // Workspace root
    pub shell_manager: SharedShellManager, // Shell process manager
    pub trust_mode: bool,               // Trust external paths
    pub auto_approve: bool,             // YOLO mode
    pub sandbox_policy: SandboxPolicy,   // Sandbox policy
    pub features: Features,             // Feature flags
    pub network_policy: Option<NetworkPolicyDecider>, // Network policy
    pub lsp_manager: Option<Arc<LspManager>>,  // LSP manager
    pub large_output_router: Option<LargeOutputRouter>, // Large output router
    pub cancel_token: Option<CancellationToken>, // Cancellation signal
    pub tool_progress: Option<Arc<dyn ToolProgressEmit>>, // Streaming progress
    // ...
}
```

### 1.3 Tool Registration: `ToolRegistry` / `ToolRegistryBuilder`

```rust
// Typical Agent mode registration
ToolRegistryBuilder::new()
    .with_full_agent_surface(client, model, manager, runtime, allow_shell, todo, plan)
    .build()
```

`with_full_agent_surface` expands to all tool families: file, search, shell, git, web, patch, task, sub-agent, todo, plan, review, RLM, RecallArchive, etc.

### 1.4 TaskType Tool Surface Trimming (Office / Code)

`TaskType` (`crates/runtime-server/src/task_type.rs`) trims tools at the **registration layer**, not only via prompt prohibitions. Aligns with [task-type-prompt-architecture.md](../task-type-prompt-architecture.md) §4.2 matrix.

| Builder method | TaskType | Description |
|----------------|----------|-------------|
| `with_office_surface(include_web)` | Office | read/list/`file_info`, `write_file`, `write_office`, `glob_files` + `file_search` (**no** `grep_files`), `load_skill`, `note`, `request_user_input`; adds `with_web_tools()` when `include_web` |
| `with_agent_tools(allow_shell)` + `with_full_agent_surface(...)` | Code | Full Agent surface: file write, `edit_file`/`apply_patch`, search trio, `grep_files`, Git, diagnostics, `run_tests`, task/automation, Web quartet; Shell family when `allow_shell` |

**Search trio (Code only):** `glob_files` → `grep_files` → `read_file` (see `crates/runtime-server/src/prompts/base.md`). Office uses only path discovery from the first two (`glob_files`, `file_search`); content search does not use `grep_files`, and **`exec_shell` running `grep`/`rg` is forbidden**.

**Web family (`with_web_tools`):** Always registers `web_search`, `fetch_url`, `finance`, `web.run`; Office uses the same set when networking is enabled.

---

## 2. Tool Execution Flow

```
① LLM returns tool_calls ──▶ ② Engine dispatches to ToolRegistry
                                  │
                    ┌─────────────┴─────────────┐
                    ▼                           ▼
           ③ Approval check              ④ Pre-execution validation
         (approval_requirement)         (path safety, sandbox policy)
                    │                           │
                    └───────────┬───────────────┘
                                ▼
                    ⑤ ToolSpec::execute(input, context)
                                │
                    ┌───────────┴───────────┐
                    ▼                       ▼
            ⑥ ToolResult            ⑦ Post-processing pipeline
           { content,                 ├─ LSP diagnostic injection
             success }                ├─ Unified diff output
                                      └─ Large output routing (#548)
```

Tool approval/resolution over HTTP/SSE: see [API_DESIGN.md](./API_DESIGN.md) §3.2.1 (`approval.required`) and §3.3.4 (`resolve-approval`).

---

## 3. Tool Reference

### 3.1 File Operation Tools

Source: `crates/runtime-server/src/tools/file.rs` (~2249 lines)

#### `read_file` — Read File

| Property | Value |
|----------|-------|
| Capabilities | `ReadOnly`, `Sandboxable` |
| Approval | `Auto` |
| Parallel | ✅ Yes |

**Behavior:**

1. **Path resolution:** `context.resolve_path(path_str)` — normalize relative path to absolute, scoped to workspace (unless `trust_mode` or path in `trusted_external_paths`)
2. **Format detection:** by extension:
   - `.pdf` → `pdftotext` or `pdf-extract` external command
   - `.docx` / `.xlsx` / `.pptx` → OOXML ZIP extract plain text
   - Plain text → streaming newline decode (low memory)
3. **Newline handling:** auto-detect UTF-16/UTF-32 BOM, convert to UTF-8
4. **Paged read:** `start_line` (1-based) + `limit` (default 2000, max 5000)
5. **Size limits:** reject >100MB; skip line count >10MB

#### `write_file` — Write/Create File

| Property | Value |
|----------|-------|
| Capabilities | `WritesFiles`, `Sandboxable`, `RequiresApproval` |
| Approval | `Suggest` |

**Behavior:**

1. Auto-create parent dirs (`create_dir_all`)
2. Snapshot before write; unified diff after (`make_unified_diff`)
3. Call `lsp_diagnostics_for_paths()` after edit
4. Output: `[diff] + Created/Wrote N bytes to path + [LSP diag]`

#### `edit_file` — Precise Text Replacement

| Property | Value |
|----------|-------|
| Capabilities | `WritesFiles`, `Sandboxable`, `RequiresApproval` |
| Approval | `Suggest` |

**V0 → V4 evolution** (see `docs/edit-file-v*.md`):

**Four operation modes:**

| Mode | Required params | Description |
|------|-----------------|-------------|
| `search_replace` | `path, search, replace` | Search-replace; multiple matches need `replace_mode: "first"|"all"` |
| `insert_after` | `path, text, after_line` | Insert after line; `after_line=0` = file start |
| `delete_lines` | `path, start_line, end_line` | Delete line range; `dry_run: true` preview |
| `replace_line` | `path, line, text` | Replace single line (bypass search match) |

**Key safety mechanisms:**

1. **E1 newline normalization:** Windows CRLF auto-detect; search string adapted
2. **E2 line range bounds:** `start_line`/`end_line` limit search region
3. **E3 diagnostic errors:** detailed hints on miss ("file uses CRLF", "confirm indent for multiline", "use `grep_files` to locate")
4. **E4 ambiguity guard:** reject >1 match without explicit `replace_mode`
5. **V1-4 compact output:** ≤5 line search/replace attaches compact before/after
6. **JSX balance check:** `.tsx`/`.jsx` brace/paren balance

**Output format:**
```
[diff unified]
Replaced N occurrence(s) in path (line X, line Y) — file now Z lines
--- compact ---
  - old line
  + new line
<diagnostics file="path"> … </diagnostics>
```

#### `list_dir` — List Directory

| Capability | `ReadOnly`, `Sandboxable` |
| Approval | `Auto` |

Returns `{ entries: [{ name, kind: "file"|"dir", size? }] }`, sorted by name, dirs first.

#### `file_info` — File Metadata

| Capability | `ReadOnly`, `Sandboxable` |

Reads size, mtime, text/binary sniff, PDF detection, line count (≤10MB), encoding guess.

---

### 3.2 Search Tools

Code search trio (recommended order): `glob_files` → `grep_files` → `read_file`

#### `glob_files` — Glob File Find

Source: `crates/runtime-server/src/tools/glob_files.rs` (~214 lines)

| Capability | `ReadOnly`, `Sandboxable` |
| Approval | `Auto` |
| Parallel | ✅ |

**Main params:** `pattern` (required, e.g. `**/*login*.{ts,tsx}`), `path` (relative to workspace root, default `.`), `limit` (default/max 100), `respect_gitignore` (default true)

**Behavior:**

1. `globset` compile pattern; walk workspace (`workspace_walk::collect_workspace_files`)
2. Sort by mtime (newest first), max 100
3. Filename discovery; fuzzy paths → `file_search`; content → `grep_files` (Code only)

#### `grep_files` — Regex Code Search

Source: `crates/runtime-server/src/tools/search.rs` (~1095 lines)

| Capability | `ReadOnly`, `Sandboxable` |
| Approval | `Auto` |
| Parallel | ✅ |

**Behavior:**

1. Compile user regex with Rust `regex` crate
2. Walk workspace; skip files >10MB (treated as binary)
3. `include`/`exclude` glob filters
4. Each match: `{ file, line_number, line, context_before, context_after }`
5. Default max 100 results; configurable `max_results`
6. `case_insensitive`, `context_lines`
7. **Symbol index** (`symbol_index`: true): query prebuilt index for definitions (`symbol_index_hits`); built by `syn` parser (`fn`/`struct`/`enum`/`trait`/`impl`, etc.)

#### `file_search` — Fuzzy Filename Search

Source: `crates/runtime-server/src/tools/file_search.rs` (~322 lines)

| Capability | `ReadOnly`, `Sandboxable` |
| Approval | `Auto` |

**Behavior:**

1. `ignore` crate `WalkBuilder` (respects `.gitignore`)
2. **Fuzzy scoring** on filename/path segments (consecutive character match)
3. `extensions` filter (e.g. `["rs", "md"]`)
4. Default 20 results, max 200
5. Returns `{ path, name, score }`

---

### 3.3 Shell Execution Tools

Source: `crates/runtime-server/src/tools/shell.rs` (~2560 lines)

#### `exec_shell` — Command Execution

| Capability | `ExecutesCode`, `Sandboxable`, `RequiresApproval` |
| Approval | `Required` |

**Two execution modes:**

| Mode | Trigger | Behavior |
|------|---------|----------|
| **Foreground** | `background: false` (default) | Block until complete; kill on timeout. Default 120s, max 600s |
| **Background** | `background: true` or `tty: true` | Return `task_id` immediately; poll via `exec_shell_wait` |

**Key parameters:**

| Parameter | Description |
|-----------|-------------|
| `command` | Required shell command |
| `timeout_ms` | Timeout (default 120000, max 600000) |
| `background` | Background mode |
| `interactive` | Interactive (allocate PTY) |
| `stdin` | Data pre-sent to stdin |
| `cwd` | Working directory (default: workspace root) |
| `tty` | PTY allocation (implies background) |

**Security layers:**

1. **ExecPolicy engine:** optional command safety analysis (`crates/execpolicy/`), user policy file pre-check
2. **Sandbox:** macOS Seatbelt / Windows native sandbox (`crates/windows-sandbox/` + `runtime-server/src/sandbox/`)
3. **Timeout enforcement:** `wait_timeout` kill
4. **Output truncation:** `shell_output::truncate_with_meta` — stdout/stderr truncated to safe length, original byte count preserved

**Return structure** (`ShellResult`):

```json
{
  "task_id": "uuid",
  "status": "Completed",
  "exit_code": 0,
  "stdout": "...",
  "stderr": "...",
  "duration_ms": 1234,
  "stdout_len": 50000,
  "stdout_omitted": 0,
  "stdout_truncated": false,
  "sandboxed": false
}
```

#### Shell Management Family

| Tool | Purpose |
|------|---------|
| `exec_shell_wait` / `exec_wait` | Poll background task output (block until done or timeout) |
| `exec_shell_interact` / `exec_interact` | Send stdin to running background task |
| `exec_shell_cancel` | Terminate specified background task |
| `task_shell_start` / `task_shell_wait` | Durable shell tasks (linked to durable task) |

**`ShellManager`** (`SharedShellManager`) shared backend:

- `HashMap<String, BackgroundShell>` — all active shell jobs
- **Checkpoint/resume** (process snapshot)
- **Stale job memory:** timeout/killed job results retained for later query
- **Sandbox policy** injection

---

### 3.4 Git Tools

Source: `crates/runtime-server/src/tools/git.rs`, `git_history.rs`

| Tool | Capability | Description |
|------|------------|-------------|
| `git_status` | `ReadOnly` | Parse `git status --porcelain` → `{ staged, unstaged, untracked }` |
| `git_diff` | `ReadOnly` | `git diff` (`--staged`, file filter, context lines) |
| `git_log` | `ReadOnly` | `git log --oneline`, `max_count` |
| `git_show` | `ReadOnly` | `git show <commit>` |
| `git_blame` | `ReadOnly` | `git blame <file>`, line range |

All `ReadOnly` + `Auto` approval.

---

### 3.5 Patch Tools

Source: `crates/runtime-server/src/tools/apply_patch.rs` (~1489 lines)

#### `apply_patch` — Apply Unified Diff

| Capability | `WritesFiles`, `Sandboxable`, `RequiresApproval` |
| Approval | `Suggest` |

**Behavior:**

1. **Parse unified diff:** multi-file, multi-hunk
2. **Fuzzy match:** when exact line mismatch, search context within ±50 lines (`MAX_FUZZ`)
3. **Create/delete files:** recognize `--- /dev/null` and `+++ /dev/null`
4. **Per-hunk report:** `{ hunks_applied, hunks_total, fuzz_used, hunks_with_fuzz }`
5. **Atomicity:** write only if all hunks succeed, else rollback
6. Output includes `touched_files`, triggers LSP diagnostics

**Output:**
```json
{
  "success": true,
  "files_applied": 2,
  "files_total": 3,
  "hunks_applied": 5,
  "hunks_total": 5,
  "fuzz_used": 1,
  "touched_files": ["src/main.rs", "src/lib.rs"],
  "message": "..."
}
```

---

### 3.6 Web Tools

Source: `web_search.rs` (~706 lines), `fetch_url.rs` (~509 lines), `web_run.rs`

#### `web_search` — Web Search

| Capability | `ReadOnly`, `Network`, `Sandboxable` |
| Approval | `Auto` |

**Behavior:**

1. **Prefer DuckDuckGo** (`html.duckduckgo.com`): HTML request, regex parse `<a class="result__a">` + `<div class="result__snippet">`
2. **Bing fallback:** when DDG unreachable (network/TLS/DNS/timeout/non-200), try `www.bing.com`
3. **Result cap:** 5 per page (`DEFAULT_MAX_RESULTS`), up to 10 (`MAX_RESULTS`)
4. **HTML cleanup:** strip tags/scripts/styles to plain text
5. **Network policy:** `NetworkPolicyDecider` (`Deny` / `Prompt` / `Allow`)
6. **User-Agent:** Safari-like

**Output:** `{ query, source, count, results: [{ title, url, snippet }] }`

#### `fetch_url` — Direct HTTP Fetch

| Capability | `ReadOnly`, `Network`, `Sandboxable` |
| Approval | `Auto` |

**Behavior:**

1. HTTP GET, redirects (max 5)
2. **Format:** `markdown` (default, HTML→text), `text`, `raw`
3. **Size limit:** default 1MB, hard cap 10MB → `truncated: true`
4. Timeout default 15s, max 60s
5. HTML cleanup: strip `<script>`, `<style>`, tags, collapse whitespace

#### `web.run` — Browser Automation

Source: `web_run.rs` (~1826 lines)

| Capability | `ReadOnly`, `Network`, `Sandboxable` |
| Approval | `Auto` |

Tool name `web.run` (dot-separated, matches OpenAI tool naming). Single call can batch sub-commands; structured results with `ref_id`.

**Sub-command array** (all optional, at least one):

| Field | Purpose |
|-------|---------|
| `search_query` | Web search (`q`, optional `recency`/`max_results`/`domains`) |
| `image_query` | Image search |
| `open` | Open cached page by `ref_id` (optional `lineno`) |
| `click` | Click element (`ref_id` + `id`) |
| `find` | In-page text search |
| `screenshot` | Page screenshot (base64) |

**Session state:** in-process `WEB_RUN_STATE` namespace → page cache; TTL 30 min, max 64 namespaces, 256 pages per namespace. Network policy per URL host (decider key also `web_run`).

#### `finance` — Market Quotes

Source: `finance.rs` (~951 lines)

| Capability | `ReadOnly`, `Network`, `Sandboxable` |
| Approval | `Auto` |

**Behavior:**

1. Tool name `finance`; stock/crypto quotes (Yahoo Finance-style public endpoints)
2. Prefer `quote` API; fallback `chart` (5-day range)
3. Override via `DEEPSEEK_FINANCE_QUOTE_BASE_URL` / `DEEPSEEK_FINANCE_CHART_BASE_URL`
4. Default timeout 10s, max 60s

---

### 3.7 Sub-Agent Tools

Source: `crates/runtime-server/src/tools/subagent/mod.rs` (~4351 lines)

Sub-agents let the model **recursively dispatch subtasks** to isolated Agent instances with restricted tool sets and context windows.

| Tool | Function |
|------|----------|
| `agent_spawn` / `spawn_agent` / `delegate_to_agent` | Create and start sub-agent (`type`, `prompt`) |
| `agent_result` | Get completed sub-agent result |
| `agent_wait` / `wait` | Block until sub-agent completes |
| `agent_list` | List all sub-agent states |
| `agent_cancel` / `close_agent` | Cancel running sub-agent |
| `agent_send_input` / `send_input` | Send input to running sub-agent |
| `agent_assign` / `assign_agent` | Bind sub-agent to task ID |
| `agent_resume` / `resume_agent` | Resume paused sub-agent |

**Sub-agent types** (`SubAgentType`):

| Type | Tool set | Purpose |
|------|----------|---------|
| `general` | Full Agent surface | General multi-step tasks (default) |
| `explore` | Read-only file + search | Fast codebase exploration |
| `plan` | Analysis tools | Architecture planning |
| `review` | Read-only file + search + Git | Code review, scoring |
| `implementer` | File + Shell + Patch + Git | Focused implementation |
| `verifier` | Shell (`cargo test`, etc.) | Run tests, report pass/fail |
| `auditor` | Read-only file + search | Mechanical fact check (file_path + line_number + symbol) |
| `custom` | Specified at spawn | Custom tool set |

**Key mechanisms:**

- **Concurrency cap:** `[subagents].max_concurrent` in config.toml (default 10, hard cap 20)
- **Recursion depth:** `DEFAULT_MAX_SPAWN_DEPTH` limits nesting
- **File leases** (`RESIDENT_LEASES`): one resident sub-agent write lease per file at a time
- **Cancel propagation:** parent `CancellationToken` passed to all sub-agents
- **Structured verdict:** Reviewer returns `structured_verdict { PASS, BLOCKER, MAJOR, FAIL }`, triggers auto fix-loop

CRAFT SSE events (`craft.verdict`, `craft.board_updated`): see [API_DESIGN.md](./API_DESIGN.md) §3.2.1.

---

### 3.7.1 Task vs Sub-Agent (Do Not Conflate)

| | **Sub-agent** (`agent_*`) | **Task** (`task_*`) |
|---|---------------------------|---------------------|
| Relationship | Subordinate **dispatched** by main agent (`spawn_depth`, parent cancel cascade, tools narrowed by type) | **Resumable background job**, **parallel** to main session thread (TaskManager worker runs independent turn) |
| Typical use | Partitioned explore, review, implementer/auditor roles | Long-running tickets, cross-session resume, gate/PR flows |
| Result to main session | `agent_result` / `agent_wait`; may inject `subagent.done` without tool | Must actively **`task_read`** (no auto sentinel) |
| CRAFT `task_id` | `agent_spawn(..., task_id=…)` writes blackboard | Same key name; **does not require** `task_create` |

Repo-wide audit parallel zones: use **sub-agent**, not `task_create` as fake “sub-agent”. See [§3.7.1 Task vs Sub-Agent](#371-task-vs-sub-agent-do-not-conflate).

HTTP task endpoints: [API_DESIGN.md](./API_DESIGN.md) §3.3.5.

---

### 3.8 Task Tools

Source: `tasks.rs` + `automation.rs` + `github.rs`

| Tool | Function |
|------|----------|
| `task_create` | Create durable background task (cross-session resume) |
| `task_list` | List recent tasks |
| `task_read` | Task detail (timeline, checklist, gate evidence, artifact) |
| `task_cancel` | Cancel task |
| `task_gate_run` | Run verification gate (`fmt` / `check` / `clippy` / `test` / `custom`) |
| `task_shell_start` / `task_shell_wait` | Shell task linked to durable task |
| `github_issue_context` / `github_pr_context` | Read-only GitHub issue/PR |
| `github_comment` / `github_close_issue` | Write ops (approval + evidence) |
| `pr_attempt_record` / `pr_attempt_list` / `pr_attempt_read` / `pr_attempt_preflight` | PR attempt management |
| Automation series | `automation_create` / `automation_list` / `automation_read` / `automation_update` / `automation_run` / `automation_pause` / `automation_resume` / `automation_delete` — 8 automation tools |

---

### 3.9 Plan & Todo Tools

| Tool | Source | Function |
|------|--------|----------|
| `update_plan` | `plan.rs` | Update high-level strategy plan (3–6 phases with status) |
| `checklist_write` / `checklist_add` / `checklist_update` / `checklist_list` | `todo.rs` | Fine-grained checklist |
| `todo_write` / `todo_add` / `todo_update` / `todo_list` | `todo.rs` | Alias compatibility |

**Design principle:** `update_plan` = strategic (high-level phases); `checklist_write` = tactical (verifiable leaf tasks). Engine syncs to UI sidebar.

---

### 3.10 Analysis & Review Tools

| Tool | Function |
|------|----------|
| `review` | Independent LLM code change review |
| `rlm` (Recursive Language Model) | Recursive chunk processing of oversized input in Python REPL sandbox; helpers `llm_query` / `llm_query_batched` / `rlm_query` |
| `diagnostics` | Workspace diagnostics: Git status, Rust toolchain, sandbox availability |
| `project_map` | Generate project structure map |

#### `rlm` Behavior

1. Load input into Python REPL as `PROMPT`
2. Sub-LLM writes Python: chunk input → `llm_query()` per chunk → aggregate
3. `llm_query_batched` for parallel chunks
4. Original input text **does not enter** caller context window (saves tokens)

---

### 3.11 Other Tools

| Tool | Function |
|------|----------|
| `write_office` | Generate `.xlsx` / `.docx` / `.pptx` (XLSX pure Rust; DOCX/PPTX via Python) |
| `load_skill` | Load skill definition from `SKILL.md` into context |
| `describe_image` | Vision model image-to-text (SiliconFlow Qwen3-VL) |
| `validate_data` | Validate JSON/TOML |
| `note` | Append persistent note to `.deepseek/notes.md` |
| `remember` | Write user memory file (#489) |
| `recall_archive` | Search checkpoint-restart archive (#127) |
| `revert_turn` | Revert last turn edits (workspace snapshot side-repo) |
| `run_tests` | Run cargo test |
| `request_user_input` | Ask user 1–3 multiple-choice questions |
| `fim_edit` | Fill-in-the-Middle edit (requires DeepSeekClient) |

---

## 4. Security Mechanisms

### 4.1 Path Safety: `context.resolve_path()`

```rust
pub fn resolve_path(&self, path_str: &str) -> Result<PathBuf, ToolError>;
```

1. **Normalize:** `Path::new(path).canonicalize()` resolves `..` and symlinks
2. **Workspace bound:** normalized path must be under `workspace` prefix
3. **Trust exemption:** `trust_mode` or `trusted_external_paths` may escape workspace
4. **Reject escape:** path escape → `ToolError::PathEscape`

Sandbox capability matrix: [SANDBOX_CAPABILITY_MATRIX.md](./SANDBOX_CAPABILITY_MATRIX.md).

### 4.2 Approval System

| Level | Trigger | Behavior |
|-------|---------|----------|
| `Auto` | `ReadOnly` tools | Execute directly |
| `Suggest` | `WritesFiles` tools | Show confirm prompt; user may skip |
| `Required` | `ExecutesCode` tools | Must manually approve |

**Approval flow:** Engine pauses before execute → SSE `approval.required` → user responds via `POST /v1/threads/{id}/turns/{turn_id}/resolve-approval` → Engine continues or rejects. See [API_DESIGN.md](./API_DESIGN.md) §7.2.

**YOLO mode:** `ToolContext::auto_approve = true` skips all approval; still bound by ExecPolicy and sandbox.

### 4.3 ExecPolicy Engine

`crates/execpolicy/` command safety analysis:

- User policy file for allow/deny patterns
- `exec_shell` evaluated before sandbox
- Can block dangerous commands (`rm -rf /`, `curl | sh`, etc.)

### 4.4 Sandbox

> **Note:** `ToolContext::sandbox_policy` is `spec::SandboxPolicy`, currently only `None` (default no sandbox). Actual sandbox enum is `crate::sandbox::SandboxPolicy`, injected by Shell at execute time. Different semantics: `spec::SandboxPolicy` = tool context layer; `crate::sandbox::SandboxPolicy` = process isolation layer.

Sandbox module `crates/runtime-server/src/sandbox/` (Windows enforcement in `crates/windows-sandbox/`):

- **macOS:** Seatbelt (`sandbox-exec`) — filesystem, network, process spawn limits
- **Windows:** Native sandbox — **unelevated:** restricted token + cap-SID workspace ACL (write isolation; no profile read isolation). **Elevated (recommended):** dedicated sandbox users + grant-exclusion/deny-read + WFP per-SID network block + command-runner IPC (sync, background, ConPTY). Setup: `zagens sandbox setup`. See [`SANDBOX_CAPABILITY_MATRIX.md`](./SANDBOX_CAPABILITY_MATRIX.md).
- **Linux:** planned seccomp/landlock — currently env marker only (`DEEPSEEK_SANDBOX_UNENFORCED`)

### 4.5 Network Policy

`NetworkPolicyDecider` controls `web_search` / `fetch_url` by domain:

- `Allow`: permit
- `Deny`: reject
- `Prompt`: user approval required

---

## 5. Post-Processing Pipeline

### 5.1 LSP Diagnostic Auto-Injection

`lsp_diagnostics_for_paths()` runs after every `write_file` / `edit_file` / `apply_patch`:

```
Edit complete → async start LSP server → run diagnostics on edited files
→ append results as <diagnostics> block to ToolResult.content
```

**Format:**
```
<diagnostics file="crates/runtime-server/src/foo.rs">
  ERROR [12:8] missing semicolon
  WARNING [45:1] unused variable `x`
</diagnostics>
```

Agent receives diagnostics as synthetic user message next turn; can fix compile errors without manual `cargo check`.

Supported LSP:
- **Rust:** rust-analyzer
- **TypeScript/TSX:** typescript-language-server
- **Python:** pyright
- **Go:** gopls
- **C/C++:** clangd

### 5.2 Large Output Routing (#548)

`LargeOutputRouter` intercepts ToolResult >4096 tokens:

1. **Threshold estimate:** conservative ~3 chars/token
2. **Extractive synthesis:** evidence ledger + high-signal lines + head/tail preview (Flash live call remains optional when a client is safe at the registry layer)
3. **Raw storage:** full output in `workshop_vars["last_tool_result"]`
4. **Promotion:** Agent calls `promote_to_context` (peek / max_chars) for full output in context

**Bypass:** pass `raw=true` on tool call to skip routing.

### 5.2.1 Evidence envelope

Search / edit / write tools attach `metadata.evidence`:

| Field | Meaning |
|-------|---------|
| `facts[]` | Machine facts (`path`, `match_count`, `edited`, …) |
| `citations[]` | `path` + optional line range |
| `uncertainty` | `none` \| `not_found` \| `partial` \| `truncated` |

`compact_tool_result_for_context` **keeps the evidence ledger** when truncating prose. Prefer intent tools `investigate` / `change_and_verify` over long primitive chains. Soft Agent phase (Explore → Edit → Verify → Ship) eagerly exposes phase-relevant tools without hard-blocking the rest.

### 5.2.2 Differential `read_file`

Prefer `around_line` / `symbol` + `context_radius` over re-reading whole files after a prior hit.

### 5.3 Unified Diff Output

`make_unified_diff()` uses `similar` crate `TextDiff` for git-style diff. Agent sees line-level `+`/`-` changes, not single-line summaries.

---

## 6. Tool Inventory (~80 tools)

### File ops (5)
`read_file` `write_file` `edit_file` `list_dir` `file_info`

### Search (3)
`grep_files` `glob_files` `file_search`

### Shell (8)
`exec_shell` `exec_shell_wait` `exec_shell_interact` `exec_wait` `exec_interact`
`task_shell_start` `task_shell_wait` `exec_shell_cancel`

### Git (5)
`git_status` `git_diff` `git_log` `git_show` `git_blame`

### Patch (1)
`apply_patch`

### Web (4)
`web_search` `fetch_url` `web.run` `finance`

### Sub-agents (8)
`agent_spawn` `agent_result` `agent_wait` `agent_list` `agent_cancel`
`agent_send_input` `agent_assign` `agent_resume`

### Tasks & automation (15+)
`task_create` `task_list` `task_read` `task_cancel` `task_gate_run`
`github_issue_context` `github_pr_context` `github_comment` `github_close_issue`
`pr_attempt_record` `pr_attempt_list` `pr_attempt_read` `pr_attempt_preflight`
`automation_create` `automation_list` `automation_read` `automation_update` `automation_run` `automation_pause` `automation_resume` `automation_delete` (8)

### Plan & Todo (8)
`update_plan` `checklist_write` `checklist_add` `checklist_update` `checklist_list`
`todo_write` `todo_add` `todo_update` `todo_list`

### Intent composites (evidence-stamped)
`investigate` (wraps `explore_codebase`) `answer_from_repo` (cite-or-refuse) `change_and_verify` (wraps `edit_and_check`)  
T5 primitives remain: `explore_codebase` `edit_and_check`

### Analysis & review (4)
`review` `rlm` `diagnostics` `project_map`

### Other (10+)
`write_office` `load_skill` `describe_image` `validate_data` `note`
`remember` `recall_archive` `revert_turn` `run_tests` `request_user_input`
`fim_edit` `promote_to_context`

### Deferred / engine-level tools

Not registered by `ToolRegistryBuilder`; managed by engine layer (`crates/core/src/engine/tool_catalog.rs`):

`tool_search_tool_regex` `tool_search_tool_bm25` — deferred tool discovery (regex/BM25 over registered but unexposed tools)

`code_execution` — reserved name (OpenAI `code_execution_20250825` compat); not active

### MCP tools (deferred loading)
MCP tool names generated dynamically from `mcp.json` servers; registered with `defer_loading=true`. HTTP MCP config: [API_DESIGN.md](./API_DESIGN.md) §3.3.8.

---

## 7. Source File Index

Line counts are repo snapshot (2026-05-18); verify with `wc`; large files for navigation only.

| Component | File | Lines |
|-----------|------|-------|
| Tool abstraction | `crates/tools/src/lib.rs` | 469 |
| ToolSpec trait + ToolContext | `crates/runtime-server/src/tools/spec.rs` | 791 |
| Tool registration | `crates/runtime-server/src/tools/registry.rs` | 1395 |
| File tools | `crates/runtime-server/src/tools/file.rs` | 2249 |
| Search tools | `crates/runtime-server/src/tools/search.rs` | 1095 |
| File glob | `crates/runtime-server/src/tools/glob_files.rs` | 214 |
| Shell tools | `crates/runtime-server/src/tools/shell.rs` | 2560 |
| Patch tools | `crates/runtime-server/src/tools/apply_patch.rs` | 1489 |
| Web search | `crates/runtime-server/src/tools/web_search.rs` | 706 |
| URL fetch | `crates/runtime-server/src/tools/fetch_url.rs` | 509 |
| Web browse | `crates/runtime-server/src/tools/web_run.rs` | 1826 |
| Finance | `crates/runtime-server/src/tools/finance.rs` | 951 |
| Sub-agents | `crates/runtime-server/src/tools/subagent/mod.rs` | 4351 |
| Diff format | `crates/runtime-server/src/tools/diff_format.rs` | 77 |
| Large output router | `crates/runtime-server/src/tools/large_output_router.rs` | — |
| Evidence envelope | `crates/tools/src/evidence.rs` | — |
| Citation auditor | `crates/core/src/engine/citation_auditor.rs` | — |
| Diff-read anchors | `crates/core/src/engine/diff_read_anchors.rs` | — |
| Intent composites | `crates/runtime-server/src/tools/intent_tools.rs` | — |
| Promote large output | `crates/runtime-server/src/tools/promote_to_context.rs` | — |
| Flash large-output synthesizer | `crates/runtime-server/src/tools/large_output_synthesizer.rs` | — |
| Agent soft phase | `crates/core/src/engine/agent_tool_phase.rs` | — |
| Diagnostics | `crates/runtime-server/src/tools/diagnostics.rs` | 251 |
| Filename search | `crates/runtime-server/src/tools/file_search.rs` | 322 |
| TaskType | `crates/runtime-server/src/task_type.rs` | — |

---

## 8. Related Documentation

| Document | Content |
|----------|---------|
| [API_DESIGN.md](./API_DESIGN.md) | Dual-channel API, SSE events, tool approval HTTP |
| [RUNTIME_ARCHITECTURE.md](./RUNTIME_ARCHITECTURE.md) | Sidecar stack, turn loop, tool host placement |
| [task-type-prompt-architecture.md](../task-type-prompt-architecture.md) | TaskType prompt + tool surface matrix |

---

## 9. Innovation roadmap status (2026-07)

Status of the **evidence envelope + intent composites + context economy** iteration (plan: evidence / catalog / large-output).  
Legend: **Shipped** = in tree and wired; **Partial** = usable but incomplete vs plan; **Not shipped** = still open.

### 9.1 M1 — Evidence envelope (anti-hallucination)

| Item | Status | Notes / code |
|------|--------|----------------|
| `ToolResult.metadata.evidence` (`facts` / `citations` / `uncertainty`) | **Shipped** | `crates/tools/src/evidence.rs`; helpers on `ToolResult` |
| Emit evidence from `read_file` / `grep_files` / `glob_files` / `edit_file` / `write_file` | **Shipped** | File + search + glob families |
| Compact / small results keep evidence ledger; truncate → no-invention hint | **Shipped** | `crates/core/src/engine/context.rs` (ledger always prepended when present) |
| Engine cheap verify of citations (line in file, match count vs prose) | **Shipped** | `citation_auditor.rs` + registry `annotate_citation_audit` (FS line count + match hint) |
| Verbal claim vs recent N facts (inject “unverified”) | **Shipped** (soft) | `claim_evidence.rs` + `no_tool_uses` one-shot path-claim nudge |
| Evidence on `exec_shell` / `web_search` / `fetch_url` | **Shipped** | exit_code / URL citations / truncated+empty uncertainty |
| Edit/write file content hash in facts | **Partial** | Path / edited / edit line window / LSP presence facts; no content hash |

### 9.2 M2 — Context economy

| Item | Status | Notes / code |
|------|--------|----------------|
| Workshop store + spillover / large-output persist | **Shipped** | Pre-existing + A1 persist paths |
| Evidence-aware **extractive** synthesis on large route | **Shipped** | `LargeOutputRouter::extractive_synthesis` in registry path |
| Live V4-Flash synthesis call from registry | **Shipped** (host callback) | `LargeOutputSynthesizer` + `FlashLargeOutputSynthesizer`; registry falls back to extractive |
| `promote_to_context` tool (peek / max_chars / consume) | **Shipped** | `crates/runtime-server/src/tools/promote_to_context.rs` |
| Differential `read_file` (`around_line` / `symbol` / `context_radius`) | **Shipped** | Schema + `resolve_read_window` |
| Differential `read_file` (`around_last_edit` / `since_tool_use_id`) | **Shipped** | Session `DiffReadAnchors`; registry records metadata evidence same-turn; paths normalized workspace-relative |
| Graded noisy-tool compact (shell/web/grep/explore/browser…) | **Shipped** | `tool_result_is_noisy` + `ToolSpec::is_noisy` + `metadata.context_noisy` |
| `uncertainty=truncated` hard hint on compact / wrap | **Shipped** | Compact + `wrap_synthesis` |

### 9.3 M3 — Intent tools + phase catalog

| Item | Status | Notes / code |
|------|--------|----------------|
| `investigate` (evidence-stamped explore composite) | **Shipped** | Merges `explore_codebase` citations into evidence pack + ledger prefix |
| `change_and_verify` (edit → LSP → optional tests) | **Shipped** | Merges `edit_and_check` evidence (edit path + tests_ok) |
| `answer_from_repo` (cite-or-refuse answer shape) | **Shipped** | Wraps `investigate`; first line `outcome=refuse\|limited\|ok`; evidence pack only (no NL answer) |
| Soft Agent phase catalog Explore / Edit / Verify / Ship | **Shipped** | Phase-managed tools deferred outside current bonus; UX tools stay eager |
| Hard stage-gate style blocking for daily Agent | **Not shipped** | Soft defer-eager only; harness `stage_gate` unchanged |
| Failure-type hot-start (compile fail → diagnostics, …) | **Shipped** | `FailureHotStart` prefers LSP `ERROR [line:col]`, then rustc/shell patterns |
| MCP activate + capability footprint snippet | **Not shipped** | MCP still default-defer; no footprint injection on activate |

### 9.4 Cross-cutting (plan §A–C)

| Item | Status | Notes |
|------|--------|-------|
| Tool-name fuzzy resolve / arg repair ladder | **Shipped** | Ladder repairs common stream damage; unparseable → `[arg_repair_failed]` (no silent `{}`) |
| Call-time **preflight** (missing path / empty glob → structured fix hint) | **Not shipped** | Beyond existing schema missing-field hints |
| Schema soft-fail examples (no silent `{}` success) | **Shipped** (arg path) | Unparseable args block execute; schema examples still open |
| Identical-call guard with state fingerprint | **Not shipped** | Loop guard still count-based |
| Verify recipe auto-run after writes / done-gate on facts | **Not shipped** | Harness `assert_*` / `task_gate_run` remain separate |
| Telemetry loop (`doctor --tools` → top failure modes) | **Not shipped** as product loop | Doctor/telemetry exist; no quarterly optimization wiring |

### 9.5 Explicit non-goals (unchanged)

Still intentionally **not** done (plan §D): mass tool aliases; papering over Linux sandbox with more shell tools; forcing `grep_files` onto Office surface without TaskType matrix changes.

### 9.6 Suggested next slices

1. Preflight + state-fingerprint loop guard.  
2. Edit/write content hash facts.  
3. MCP activate + capability footprint snippet.  
4. Hard stage-gate style blocking for daily Agent (optional, beyond soft defer).  
5. Telemetry loop (`doctor --tools` → top failure modes).

### 9.7 Runtime package name (dev tip)

Workspace crate directory is `crates/runtime-server/`, library name is `zagens_runtime`, Cargo package name for checks/tests is **`zagens-cli`**:

```bash
cargo check -p zagens-cli
cargo test -p zagens-cli --lib
```
