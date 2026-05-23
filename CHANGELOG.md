# Changelog

All notable changes to this project will be documented in this file.

**Update policy:** Record **every notable change** (features, fixes, docs, DS
Pick desktop, CLI/TUI, tooling) in this file—typically under `[Unreleased]`,
in the **same PR/commit** as the change when practical. Cursor agents: see
`.cursor/rules/ds-pick-repo.mdc` § Changelog.

**DS Pick** (desktop app in `crates/desktop/`) has its **own** version line:
**MAJOR.MINOR.PATCH** in **SemVer** (e.g. **v0.3.0**). Display form **vX.Y.Z**;
each numeric segment is one or more digits (e.g. `0.2.1`, `0.10.3`). This line
**does not** follow the root workspace version used by `deepseek` / `deepseek-tui`
crates (see root `Cargo.toml` `[workspace.package] version`).

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> **2026-05-19:** `[0.3.0]` consolidates work since v0.2.2 (backfilled from `git log`).
> Prefer updating `[Unreleased]` incrementally going forward.

## [Unreleased]

### Fixed

- **Runtime (A3):** `classify_error_message` recognizes DeepSeek thinking/reasoning constraint strings as `InvalidInput` (distinct from network disconnect); golden suite centralized in `deepseek-core::error_taxonomy`.
- **Desktop (approval):** `approval_policy` from system settings now drives Composer `auto_approve` on load/save; sidecar `start_turn` reads `ApprovalMode` from config (`never` / `on-request` / `auto`) instead of hardcoding Suggest.
- **Tests:** `integration_mock_llm` re-exports `deepseek_core::LlmClient` so mock trait matches P2 `async_trait` surface.

### Changed

- **Desktop (F1a):** `TerminalCard` appends `tool.progress` to xterm incrementally instead of full clear+rewrite each frame.
- **Desktop (F1b):** `MessageBubble` shows `DiffCard` while diff tools are still running when unified diff appears in streamed output.
- **Desktop (F3):** Escape stops active generation when focus is outside inputs.

### Added

- **Desktop (F3):** Skip-to-main link; `#main-content` landmark; aria on ToolCard/DiffCard/Composer; global `focus-visible` rings; expanded `prefers-reduced-motion` (sidebar/composer).
- **Runtime (PR5):** `ThreadMessageTurnPort` — `handle_thread(Message)` delegates real turn when port is wired; app-server installs `AppServerLlmTurnPort` when `api_key` is configured (legacy `queued` fallback otherwise).
- **Runtime (A1 / R-015):** Full baseline @ `10972e4` — median RSS **28.5 MB**, `-Gate` PASS; log `deliverables/runtime-baseline-full-run.log`.
- **Runtime (A1 / R-015):** `runtime-longrun-baseline.ps1` — deterministic **1.1 MB** workspace fixture for large-tool turn; `-Gate` RSS regression vs ADR baseline (+10%); CI ubuntu dry-run step.
- **Runtime (A1 / R-015):** `large_output_router` **1 MB+** boundary unit test (`synthesise_at_one_megabyte_boundary`).
- **Docs:** G2/PR5 手测签收（2026-05-23）：§0.4 health、§1 单窗、§3 双窗并行、§5.1 Stop；审批 UI 暂缓（`approval_policy` ↔ `auto_approve` 接线债）。
- **Runtime (PR5 局部):** 双 thread 并行 turn 回归测（`parallel_turns_on_two_threads_overlap_then_complete`、`sidecar_parallel_turns_on_two_threads`）；`app-server` `Message` 仍 queued 占位（注释 SSOT）。
- **Runtime (G2 门控):** `CURRENT_EVENT_SCHEMA_VERSION`；`GET /health` 与 SSE payload 暴露 `event_schema_version`（A+.4b）。
- **Runtime (tests):** A5.5 完整回放 fixture `runtime_turn_replay.jsonl`（15 步：thinking/tool/approval/完成）。
- **Docs:** `docs/tech/adr/G2_PR5_MANUAL_SMOKE_CHECKLIST.md` — G2/PR5 桌面与多窗口手测勾选清单。
- **Docs:** `docs/tech/adr/G2_GATE_ACCEPTANCE.md` — G2 自动化验收记录与维护者待办。
- **Docs:** `docs/tech/adr/P2_PR4_SESSION_HANDOFF.md` — 新窗口继续 P2 PR4 / A4.6 / R-015 的对接说明。
- **Runtime (P2 PR4 局部):** `deepseek-core::engine::{dispatch,context}`（工具 JSON/上下文预算/plan 策略）；tui 薄 re-export；`RegistryToolDispatch` 接线 `execute_tool_with_lock`；`Engine`/`turn_loop` 仍留 tui。
- **Runtime (P2 PR4 局部):** `deepseek-core::engine::approval`（`await_tool_approval` / `recv_user_input_for_tool`、泛型 `ApprovalDecision<P>`）；tui `approval.rs` 薄壳（`UserInputRequired` 事件仍 L2）；core 加 `tokio`/`tokio-util`。
- **Runtime (P2 PR4 局部):** `deepseek-core::engine::{tool_bridge,tool_progress}`（`ToolCall`↔`ToolOutput` 转换、`emit_tool_audit`、进度文案）；tui `tool_dispatch_port` / `tool_execution` 薄壳（`RegistryToolDispatch`、`InteractiveTerminalGuard`、MCP/进度仍 L2）。
- **Runtime (P2 PR4 局部):** `deepseek-core::{events,error_taxonomy,coherence,user_input,subagent}` + tui re-export（`Event`/`ErrorEnvelope`/`CoherenceState`/`UserInputRequest`/subagent 类型）；`envelope_from_llm_error` 保留 tui（`LlmError` 孤儿规则）。
- **Runtime (P2 PR4 局部):** `TurnContext`/`TurnLoopMode`/`StreamError` 迁入 `deepseek-core`；`TurnLoopHost` + `tool_phase.rs` / `streaming_phase.rs`；**`deepseek-core::engine::handle_deepseek_turn`**（generic `TurnLoopHost`）。
- **Runtime (A4.6 局部):** `engine.rs` 拆出 `types.rs`（`EngineConfig`）、`handle.rs`、`engine_new.rs`、`engine_helpers.rs`、`session_messages.rs`、`mock.rs`；`engine.rs` ~618 → **~201 行**（达 PR4 spike **< 300** 目标）。
- **Runtime (A4.6 局部):** `engine.rs` 拆出 `op_loop.rs`、`cycle_hooks.rs`、`message_handlers.rs`（`handle_send_message` / 手动 compaction）；`engine.rs` ~2177 → ~1220 行。
- **Runtime (P2 PR4 局部):** `TurnLoopToolExecutor` + `TurnLoopToolRegistry` 关联类型；`Engine` / `McpPoolHandle` 端口实现。
- **Runtime (tests):** A5.5 最小回放 fixture `tests/fixtures/runtime_turn_minimal.jsonl` + 顺序/seq 断言。
- **Runtime (P2 PR4 局部):** `deepseek-core::engine::tool_catalog`（deferral、tool search、missing-tool 文案）；tui 薄壳保留 `AppMode` 适配与 `code_execution` 子进程。
- **Docs:** `docs/tech/adr/P2_DESKTOP_TURNLOOP_SPIKE.md` — DS Pick 经 sidecar HTTP 使用 `TurnLoopHost`（tui `host_impl`），desktop crate 不链接 `Engine`。
- **Runtime (A4.6 局部):** `engine/capacity_flow/{checkpoints,observation,events,interventions,replay,persistence}.rs`；原 monolith ~985 行拆为 6 个子模块（最大 ~370 行）。
- **Runtime (A4.6 局部):** `runtime_threads/turn_control.rs`（`interrupt_turn` / `steer_turn` / `compact_thread`）；`manager.rs` ~829 → ~589 行。
- **Runtime (A4.6 局部):** `runtime_threads/thread_crud.rs`（create/list/get/update/fork/resume/seed 等）；`manager.rs` ~1673 → ~1032 行。
- **Runtime (A4.6):** `runtime_threads/engine_load.rs`（`ensure_engine_loaded`）；`routing.rs` 路由读写。
- **Runtime (tests):** `runtime_api` 测试显式隔离 `data_dir`，不再受工作区 `DEEPSEEK_RUNTIME_DIR` 污染。
- **Runtime (A4.6):** `runtime_threads/routing.rs` 自 `manager.rs` 拆出路由规则读写。
- **Runtime (A4.6):** `runtime_threads/{active,monitor}.rs` 自 `manager.rs` 拆出（LRU/活跃 turn 状态 + `monitor_turn`）；`manager.rs` ~1.8k 行。
- **Runtime (P2 PR3 局部):** `deepseek-core::engine::{StartTurnParams, TurnEnginePort}`；`RuntimeThreadManager::start_turn` 经 core 委托 `EngineHandle`（`turn_loop` 仍在 tui）。
- **Docs:** [RUNTIME_EVOLUTION_ROADMAP.md](docs/tech/RUNTIME_EVOLUTION_ROADMAP.md) **v2.0-final** — 维护者签收 §4.2（D4–D7、D9）；§17 实施后审核（2026-05-22）；[adr/RUNTIME_BASELINE.md](docs/tech/adr/RUNTIME_BASELINE.md) R-015 占位（基准填数并行）。
- **Runtime (P2 PR1 局部):** Shared types and `LlmClient` trait in `deepseek-core` (`chat`, `models`, `turn`, `compaction`, `capacity`, `workshop`, …) with `deepseek-tui` re-exports; `deepseek-tui` **lib** target (`crates/tui/src/lib.rs`).

### Changed

- **Runtime (R-003 / A4.6 阶段 3):** Extract `runtime_threads/tests.rs`；`mod.rs` ~275 行（契约测外置）。
- **Runtime (P2 PR2 局部):** Move `Session`/`SessionUsage`、`working_set`、`project_context`、`ApprovalMode`、`CycleBriefing` into `deepseek-core` with tui re-exports; `turn_loop` still in `deepseek-tui::core::engine`.
- **Runtime (R-015):** `runtime-longrun-baseline.ps1` — release sidecar (fix debug stack overflow on Windows), load repo `.env`, poll turn `in_progress` between turns; ADR full-run RSS **26.6 MB** median @ `ab4c3c4` (`deepseek-v4-pro`, 50×3 turns); dry-run p99 **0.16 ms**.
- **Runtime (R-003 / A4.6 阶段 2):** Extract `runtime_threads/manager.rs`（`RuntimeThreadManager`、LRU、routing、turn 生命周期）；`mod.rs` ~2.4k 行（契约测仍留主文件）；`manager.rs` ~2.9k 行待后续 `tests.rs` 外置。
- **Runtime (R-003 / A4.6 阶段 1):** Extract `runtime_threads/persist.rs`（`RuntimeThreadStore` + 磁盘/事件/usage 聚合）与 `events.rs`（agent rebind hints）；`mod.rs` ~5.2k 行（`RuntimeThreadManager` 仍留主文件）。
- **Runtime (R-003 / A4.5):** Extract `health.rs`、`workspace.rs`、`usage.rs`（含 routing/symbol-index）；契约测迁至 `runtime_api/tests.rs`；`mod.rs` ~500 行（达 §12.6 <800 目标）。
- **Runtime (R-003 / A4.5):** Extract `runtime_api/skills.rs`、`mcp.rs`、`automations.rs`。
- **Runtime (R-003 / A4.5 部分):** Extract `runtime_api/sessions.rs`（会话 + resume-thread 播种）与 `runtime_api/tasks.rs`（后台任务队列）。
- **Runtime (R-003 / A4.4):** Extract `runtime_api/threads.rs` — thread CRUD, turns, snapshots, workspace browse/read (~1k lines).
- **CI (R-009):** Ubuntu job runs `sidecar_contract_full_lifecycle` explicitly (`cargo test -p deepseek-tui --lib`).
- **Runtime (R-003 / A4.2–A4.3):** Extract `runtime_api/auth.rs` and `runtime_api/stream.rs` from monolith `mod.rs`.

### Fixed

- **Runtime (RLM):** `RlmLlmClient` blanket impl uses `?Sized` so `Arc<dyn LlmClient>` compiles after `LlmClient` moved to `deepseek-core`.
- **Tests:** `cargo test -p deepseek-tui --lib` green (2368 passed) — JSON-only fixtures for schema-rejection tests (SQLite migration), `read_file` metadata key `total_lines`, Windows `pwsh` shell/display_command, approval resolve `tokio::join!` + stale-turn immediate deny, mock-engine turn timeout 8s for `QueryContext` panel emit.
- **Tests:** `subagent` stub runtime wraps client in `Arc` for P2 client type.

### Changed

- **Runtime (R-003 / A4.1):** Extract `runtime_api/router.rs` (`build_router`); handlers remain in `mod.rs` for now.

## [0.4.3] - 2026-05-21

### DS Pick (desktop)

- **v0.4.3** — `deepseek-desktop`、`tauri.conf.json`、`web-ui/package.json` 与 About 面板对齐 **v0.4.3**。

### Fixed

- **DS Pick (desktop):** Fix multi-window / continued-session chat stream duplication (`看到了看到了` / `TheThe user`) — runtime SSE proxy uses `emit_to` per window; Web UI binds SSE via `getCurrentWebviewWindow().listen`; resumed turns poll `replay_only` events instead of a long-lived `GET …/events` SSE (avoids stacked `runtime_get_sse` streams); `runtime_cancel_sse` stops in-flight proxy reads on abort; `finishOnce` aborts the turn `AbortSignal` after `turn.completed`.

## [0.4.2] - 2026-05-21

### DS Pick (desktop)

- **v0.4.2** — `deepseek-desktop`、`tauri.conf.json`、`web-ui/package.json` 与 About 面板对齐 **v0.4.2**。

### Added

- **DS Pick (desktop):** True multi-window (Cursor / VS Code model) — `WebviewWindow` per project, `tauri-plugin-single-instance`, tray/menu **新建窗口**, TitleBar + **Ctrl/Cmd+Shift+N**; per-window workspace `localStorage`, session list filter + **显示全部会话**; parallel turns per `thread_id` (switch session no longer aborts other streams); terminal `emit_to` per window; approval routed via `register_window_thread` / `thread_owned_by_window`.
- **Docs:** [multi-window-plan.md](docs/desktop/multi-window-plan.md) — multi-window plan **closed** (M1–M4 shipped; M5 deferred to backlog §7.5).

## [0.4.1] - 2026-05-21

### DS Pick (desktop)

- **v0.4.1** — `deepseek-desktop`、`tauri.conf.json`、`web-ui/package.json` 与 About 面板对齐 **v0.4.1**。

### Added

- **Docs:** [workspace-directory-plan.md](docs/desktop/workspace-directory-plan.md) — workbench Directory tab phased UI/feature plan with implementation checklist (§0, §10).
- **DS Pick (web UI):** Workbench Directory tab — flat stroke icons, toolbar (up/refresh/open folder), search filter, hidden-folder toggle (`target`, `node_modules`, etc.), merged workspace path row, scrollable list, preview highlight, `WorkspaceFilesPanel` + i18n `workspaceFiles.*`.
- **DS Pick (web UI):** Workbench Directory **tree view** (phase D) — lazy-loaded `WorkspaceFileTree`, per-workspace expanded-state in `sessionStorage`, list/tree toggle with flat stroke icons.
- **DS Pick (web UI):** i18n for workbench panel — `panels.*`, `workbench.*`, `workspaceFiles.tab` / `workspaceFiles.errors.*`; `RightPanel` and workspace file open errors use `useT`.
- **DS Pick (web UI):** Tasks panel — **Clear finished** removes completed / failed / canceled records via runtime `POST /v1/tasks/clear` (queued and running tasks kept); confirmation dialog + toast.
- **DS Pick (web UI):** Workbench Files tab — workspace-wide file search (same input box, BFS via browse API; skips denylisted dirs), virtual list for large flat/search result sets (B4); chat/Diff「在目录中显示」, scroll-to-reveal, keyboard shortcuts, Office directory presets.
- **DS Pick (Phase D1):** Audit scratchpad — expandable inventory list from `scratchpad/status` `areas[]`, U1 contract violation highlight (notes without accounted areas), i18n strings; path click opens workspace preview.
- **DS Pick (web UI):** Audit scratchpad colors aligned with app theme (`bg-card`, `text-t-text`, accent/error tokens) — readable in light mode; inventory status chips match ToolCard-style badges.
- **DS Pick (Phase D2):** Scratchpad status API — `checklist_completed/total`, `contract_warnings`, findings severity tallies; audit panel dual-track (inventory vs checklist), findings strip, sub-agent active count + narrative-spawn warning; checklist tool events refresh panel.
- **DS Pick (Phase D U2):** Sidebar separates **Tasks** (`GET /v1/tasks`) and **Sub-agents** (`agent_*` SSE) into top-level inspector entries; **Skills** stays under Settings. Checklist / Tasks / Sub-agents show a **small activity dot** when there is in-flight work or unseen updates (pulse while running); opening the panel clears the indicator.
- **DS Pick (web UI):** **Usage** dashboard moved to the same sidebar tier as Checklist / Tasks / Sub-agents (display panels); Settings keeps API Key, MCP, Skills, routing, index, and system config only.
- **DS Pick (web UI):** Audit scratchpad moved from the chat composer strip to a sidebar **审计** entry and right **Audit scratchpad** panel (same behavior as Checklist); sidebar activity dot when a run is active.
- **DS Pick (web UI):** Chat **Reasoning** and **tools** sections get clipboard copy (section header + per-tool card); uses shared `copyPlainText` helper.
- **DS Pick (web UI):** Sub-agent panel shows spawn **objective**, type/role, work-package id, and progress line; runtime `agent.spawned` SSE includes `prompt`; bogus `call_*` tool-call ids are no longer listed as agents.
- **DS Pick (panel channel C):** Runtime emits `panel.scratchpad` / `panel.checklist` / `panel.context` on the live SSE stream; Web UI applies them directly and uses slow B-channel polls only as fallback while streaming.

### Changed

- **DS Pick (web UI):** Audit scratchpad panel — uniform card border on all sides (removed thick left accent stripe); attention state uses a subtle amber border tint instead.
- **DS Pick (web UI):** Remove redundant「打开文件夹」button from workbench panel header; use「在文件管理器中打开」in the Files tab toolbar instead.

### Fixed

- **DS Pick (web UI):** Workbench Files「添加至对话」/「Add to chat」 now inserts an `@` workspace path into Composer instead of opening the preview panel.
- **TUI / runtime (security, L7d P1/P2):** Session `/load` `/save` `/export` paths resolved under workspace (`path_guard`); MCP no longer trusts client `approved` for shell/write tools when `require_approval` is on; default MCP expose list is read-only (`file_read`, `search`, `file_search`); cancel token reset order fixed (H01); `Config` Debug redacts API keys; file-picker labels strip control chars; scratchpad coverage preview uses char-safe truncation; Linux/Windows sandbox types surface an explicit unenforced warning on `exec_shell` (H12); Python REPL spawns with `-I` and docs state no OS isolation (H13).
- **DS Pick (security, L7d follow-up):** P0 from `2026-05-20-001` audit — `export_*_json` validates `.json` path (no `..`/system dirs); runtime Bearer no longer exposed via `get_runtime_token` (Tauri `runtime_http` / `runtime_post_stream` / `runtime_get_sse`); Explore sub-agent `explicit_tools` intersected with read-only cap; blackboard `task_id` restricted to safe charset.
- **DS Pick (web UI):** Mermaid SVG, diff2html output, and clipboard HTML paste sanitized with DOMPurify before `innerHTML`.
- **DS Pick (web UI):** Long-audit HTTP poll storm — coalesced in-flight GETs for context/checklist/scratchpad status; longer staggered intervals while streaming; session checkpoint 60s (turn-complete + tab-hide still persist); runtime probe 18s during stream with immediate light `/health` on stream start.
- **DS Pick (runtime):** `scratchpad/status` and `checklist` handlers run on the blocking pool (2s status cache) so `/health` and SSE stay responsive under audit load.
- **DS Pick (web UI):** Sidebar「未连接」during long audits while generation still runs — periodic probe no longer requires `/v1/sessions` (2.5s timeout) while streaming; uses `/health` only so busy sidecar is not misread as offline.
- **DS Pick (web UI):** Right panel (workspace browse, MCP, tasks/skills, routing, usage) no longer hard-blocks on probe `offline` during streaming; session-list refresh failures use light probe; sidebar shows amber「繁忙（生成中）」when degraded.
- **DS Pick (web UI):** `runtimeSessionEstablished` keeps checklist, workspace, audit bar, and MCP panels on API paths after connect/resume — probe blips no longer gate panel fetches; probe requires 3 consecutive failures before `offline`; poll GETs use 45s timeout and retain last checklist/scratchpad snapshot on busy errors.

## [0.4.0] - 2026-05-20

### DS Pick (desktop)

- **v0.4.0** — `deepseek-desktop`、`tauri.conf.json`、`web-ui/package.json` 与 About 面板对齐 **v0.4.0**。

### Added

- **DS Pick (web UI):** Runtime-aligned context usage via `GET /v1/threads/{id}/context` (TUI `estimate_input_tokens_conservative` + compaction policy); Composer shows runtime estimate when sidecar is connected.
- **DS Pick (web UI):** Fix context usage indicator resetting to 0% after switching sessions and back (per-thread snapshot cache, stale refresh guard, transcript fallback when runtime snapshot is empty).
- **DS Pick (web UI):** Dual-track context display — progress ring uses conservative estimate; Composer also shows last API `input_tokens` from the provider when available.
- **TUI / runtime:** Engine records per-round API `input_tokens` (`last_api_input_tokens`); context snapshot exposes `last_api_usage_percent`; token estimate uses DeepSeek doc ratios (CJK ~0.6, ASCII ~0.3 per char).
- **DS Pick (system settings):** `[compaction]` — `auto_compact` toggle and `token_threshold` (synced to `config.toml`, shared with TUI engine compaction).
- **Audit scratchpad (Phase B):** Runtime tools `scratchpad_*`; `ScratchpadStore` + layered P2 summary injection, readonly nudge (B4), cycle handoff pointer (B3b), `ThreadRecord.scratchpad_run_id` (B2), TTL cleanup (B7), `GET /v1/threads/{id}/scratchpad/status`, DS Pick `AuditScratchpadBar` (B5). Config: `[scratchpad]` in `config.toml`.
- **Audit scratchpad (B7 hardening):** `supersedes` transitive closure; `scratchpad_append` schema tightened; per-turn single `<scratchpad_summary>`; `git_blame` counts toward readonly nudge.
- **Audit scratchpad (Phase A):** Full-repo review external memory — `pick-rules.md` §7, `base.md`, bundled **`audit-repo`** skill. Design: [audit-scratchpad-design.md](docs/desktop/audit-scratchpad-design.md).
- **Docs:** [audit-scratchpad-test.md](docs/desktop/audit-scratchpad-test.md) — Phase A smoke, resume, and **14-area** `crates/tui/src/` run (`2026-05-19-tui-src-review`).
- **Docs:** [audit-scratchpad-test.md](docs/desktop/audit-scratchpad-test.md) — Phase B smoke/gate/resume/synthesize on `2026-05-19-phase-b-smoke`; design §6 marked implemented.
- **Docs:** [audit-scratchpad-design.md](docs/desktop/audit-scratchpad-design.md) — Phase C plan §6.12 (C0–C4: compaction, coverage gate, Auditor binding, blackboard mirror); §6.10 B7 marked shipped.
- **Docs:** Phase C design review §13.5 — deferred meta gate, L0-only compact, C1/B1 boundary, Auditor track B prose check, soft-warn format.
- **Audit scratchpad (Phase C0/C1):** Compaction pin + L0-only handoff; `coverage_gate` (soft/hard); `set_area(deferred)` requires `kind=meta`; `[scratchpad]` config fields.
- **Audit scratchpad (Phase C2/C3):** `agent_spawn(type=auditor)` builds track A/B from scratchpad; blackboard `scratchpad` mirror partition; `scratchpad_run_id` spawn param.
- **Skills:** `audit-repo` — append-before-`done` ordering; bundled skills marker **v4** (`tool_search` before `write_file`; resume via `scratchpad_status`).
- **Docs:** [audit-scratchpad-design.md](docs/desktop/audit-scratchpad-design.md) §2 — product essence, philosophy (**实事求是，实践出真知**), §2.1–§2.6 (contract, onboarding brainstorm, multidisciplinary memo, short-term roadmap).
- **Docs:** [audit-scratchpad-test.md](docs/desktop/audit-scratchpad-test.md) §L7b — root-cause table and link to §14.
- **Docs:** [HARNESS.md](docs/desktop/HARNESS.md) — Agent Harness 定位（社招 JD 映射、DS Pick 栈位、会话恢复案例、与 DeepSeek 关系备忘 §7）。
- **Docs:** [audit-scratchpad-design.md §6.13](docs/desktop/audit-scratchpad-design.md) — Phase D 审计过程可视化路线图（D1–D3、产品/模型边界）；[audit-scratchpad-test.md §L7c/L8/L9](docs/desktop/audit-scratchpad-test.md) — `2026-05-20-audit` 试跑记录、可视化验收、地狱级四维暂缓。

### Fixed

- **DS Pick (web UI):** Saving system settings restarts the sidecar — UI no longer stays on「生成中」while the sidebar shows「未连接」; `sidecar://restarting` clears the stream; confirm dialog when saving during an active turn.
- **DS Pick (web UI):** Stop assistant reply body **jitter while streaming** — plain pre-wrap during tokens (Markdown after turn completes), single scroll owner (no outer 200ms poll vs inner 48vh cap), fixed-height「生成中」footer.
- **DS Pick (desktop):** Content-Security-Policy — add `font-src` (`'self'`, `data:`, dev localhost / `tauri.localhost`) so bundled UI fonts are not blocked (console CSP violation on `data:font/woff2`).

### Changed

- **DS Pick (web UI):** Audit scratchpad bar — dismiss control (×, top-right); hidden for the same thread/run until a new scratchpad run (sessionStorage).
- **DS Pick (web UI):** Audit scratchpad bar — neutral `canvas-alt` shell (aligned with tool cards); contract reminders use amber pill + left rail instead of full error red styling.
- **DS Pick (web UI):** Context ring prefers runtime snapshot over client transcript estimate; polls during streaming.
- **Audit scratchpad (L7b short-term):** Expand `[scratchpad] inject_on_report_keywords` (E1); block `write_file` to `deliverables/` audit/CODE_REVIEW paths when bound scratchpad inventory incomplete or C1 hard gate fails (E2, `scratchpad_flow::check_write_file_audit_report_gate`); **E5** — during bound audit scratchpad defer/block `task_create` and eager-load `agent_spawn` (+ join tools) so P1 parallel review uses sub-agents not TaskManager.
- **DS Pick / config:** `[subagents] step_timeout_secs` — configurable default per-step sub-agent LLM API timeout (10–600 s); system settings slider; `agent_spawn` uses it when `step_timeout_ms` is omitted (replaces hard-coded 120 s default).
- **Sub-agents / prompts:** Step API timeout errors and `base.md` / `audit-repo` spell out that omitted `step_timeout_ms` is not unlimited time; parents must re-spawn or shrink scope on timeout — not mark audit areas done.
- **Prompts / tools:** Clarify **Task** (`task_*`, peer durable work) vs **Sub-agent** (`agent_*`, parent-dispatched) in `base.md`, `tasks.rs` tool descriptions, and `agent_spawn` / `agent_result` / `agent_list` descriptions; `task_id` spawn param documented as blackboard key only.
- **DS Pick (web UI):** Custom context menu on chat workspace file links — open with system app, copy absolute path, copy relative path; suppresses the native WebView link menu (e.g. non-functional “open in new window” on `href="#"`).
- **DS Pick (web UI):** Right workbench panel **collapsed by default** on launch (left sidebar stays open); use the edge strip to expand.
- **DS Pick (web UI):** Composer **Stop** calls `POST …/turns/{turn_id}/interrupt` (runtime `engine.cancel()`), not only aborting the SSE client—matches TUI Ctrl+C / Esc interrupt semantics.
- **Audit scratchpad (discoverability):** `scratchpad_*` tools eager-loaded in Agent (not deferred); `scratchpad_status` / `scratchpad_list_notes` bind `thread.scratchpad_run_id`; `GET …/scratchpad/status` auto-discovers latest `inventory.json` when unbound (DS Pick bar). `audit-repo` skill + `base.md` / pick-rules §7: `tool_search` before `write_file` fallback.
- **DS Pick (web UI):** Audit bar shows **accounted** progress (done + in_progress + deferred), faster poll while streaming, refresh on scratchpad tool completion. **Sub-agent panel:** forward `agent.spawned` / `agent.progress` / `agent.completed` / `agent.list` on compat SSE (`POST /v1/stream`).
- **DS Pick (web UI):** Dark theme — user message text uses dedicated high-contrast `--color-msg-user-text` (fixes faint prose grays in user bubbles).
- **DS Pick (web UI):** While an assistant reply is **streaming**, the main body uses the same **48vh scroll cap** as Reasoning so CoT stays on screen; after the turn completes, the body **ease-out expands** to full height (respects `prefers-reduced-motion`).
- **DS Pick (web UI):** Sidebar / right-panel **collapse** controls move to the resize gutter—hidden until hover on the `col-resize` seam; panel-indent icon replaces header chevrons.
- **DS Pick (web UI):** Global **toast** notifications (no new npm deps) replace the chat-column amber **banner**; stack centered above the composer with success / error / warning / info variants; runtime reachability errors include **Retry connection** and auto-dismiss when the sidecar probe is healthy.
- **DS Pick (web UI):** Assistant **Reasoning** and **工具调用** blocks default to **collapsed**; click header to expand (streaming shows “推理中…” / “N 个进行中” hints while folded).
- **Docs:** [README.md](README.md) — lead with verified differentiators; split desktop vs shared runtime; trim misleading feature tables; fix dev commands (`cargo tauri dev`); align doc links (`API_DESIGN.md`, `DEV_NOTES.md`). Cursor/portable rules updated for dead links.

## [0.3.0] - 2026-05-19

### DS Pick (desktop)

- **v0.3.0** — `deepseek-desktop`、`tauri.conf.json`、`web-ui/package.json` 与侧栏标签对齐 **v0.3.0**。
- **会话回放与 diff** — 会话回放、diff 面板、上下文用量展示；Code 工作区 **交互式 PTY 终端**；切换会话时恢复上下文用量；聊天代码块复制。
- **Composer UI** — 新版输入区/composer 布局；Mermaid 渲染修复。
- **系统设置** — 完整系统设置视图迁入右侧面板；**系统托盘**（关闭窗口可隐藏到托盘）；turn 完成且窗口最小化/隐藏时 **原生桌面通知**。
- **i18n** — Web UI 国际化框架（中/英等）。
- **子代理设置** — 与 TUI 双轨对齐：`max_subagents` 优先级与 subagents 功能开关。
- **侧栏/技能** — 技能导入；Office/Code 任务类型；Documents 默认工作区；搜索类工具 UI/配置。
- **文档** — 技术文档迁至 `docs/tech/`；diff 面板暗色主题修复。

### Added

- **符号索引 V3–V5** — 懒加载按工作区构建（去除启动阻塞）；MermaidPanel 图表实时渲染；`edit_file` v0/v1；索引管理面板；符号变更追踪与 bridge 关联（V5）；调用关系与置信度（见 `docs/symbol-index-v*.md`）。
- **Sidecar 加固** — Supervisor 强化、SQLite 存储、启动门控、keyring 密钥。
- **CRAFT** — P0 结构化裁决、P1 黑板、P3 工具裁剪、fix-loop 协议（`docs/craft-v2-improvements.md`）。
- **Vision** — 默认切换 **Qwen3-VL**；模型感知提示词；代理/超时配置；`describe_image` 工具与 OCR bridge。
- **Office** — PDF 读取支持；`write_office` / xlsx 生产级修复（6 项）；工具进度流式输出与 LLM 使用指南。
- **搜索工具** — Office/Code 任务类型相关检索能力扩展。
- **提示词** — V4 幻觉抑制类系统提示调整。

### Changed

- **DS Pick** — Light theme “极淡乳白” palette (warm stone canvas/card, softer dividers, blue accent retained); center chat column uses `card` surface.
- **DS Pick** — Dark theme “深色暖 · 护眼” palette (warm gray-black shell, amber accent, softer status colors).
- **DS Pick** — Sidebar brand row: flat logo + accent “DS Pick” (no gray pill), aligned with reference mockups.
- **DS Pick** — Bundled UI fonts: Plus Jakarta Sans Variable (Latin) + Noto Sans SC (CJK); JetBrains Mono for terminal/code; replaces system Segoe/Roboto stack.
- **DS Pick** — Sidebar: app icon in brand row; title bar brand text removed; connection status at bottom (“连接正常” / “未连接”); version blurb moved to **关于** panel under Settings.
- **Runtime** — 消除 tokio worker 中的阻塞 I/O；诊断与相关文档更新。

### Fixed

- **DS Pick** — After app restart, **Reasoning** and **tool** cards restore correctly: `resume-thread` reuses persisted `runtime_thread_id` for event replay (instead of seeding a blank thread); `persist-session` stores that id; web UI mirrors UI snapshots to `localStorage` as fallback.
- **DS Pick** — Session restore `GET …/events?replay_only=1` no longer returns HTTP 400 (accept `1`/`0` query booleans; client uses `replay_only=true`).
- **DS Pick** — Switching sessions keeps in-memory UI snapshots (tools + thinking); thread event replay still refreshes from runtime when available.
- **DS Pick** — Checklist panel: sidebar **清单** entry, persist `checklist_write` on thread record (survives sidecar restart), faster poll while streaming; auto-switch no longer blocks manual **工作台** tab during streaming.
- **DS Pick** — Left sidebar width is draggable on the column edge (persisted; 180px–45% viewport); fixes broken resize handle that was absolutely positioned without a containing block.
- **DS Pick** — Unified divider tokens (`chrome-seam` vs `divider` vs `card-border`); column resize gutters use inset seams only; shell panels use tint bands instead of stacked border lines.
- **Desktop** — Unicode 工作区路径下的可点击聊天链接。
- **xlsx** — 生成路径六项生产级修复。

### Security

- CODE_REVIEW **Step 1** — 依赖与安全清理（见 `docs` 中 Step 1 跟踪）。

### Docs

- Agent 可靠性/CRAFT/符号索引/V5 等规划与实施记录更新；部分旧版 brief 归档或移除。

### Process

- **Changelog 维护** — 写入 `.cursor/rules/ds-pick-repo.mdc` 与 `project_rules.md`（notable 变更需同步本文件）。
- **Portable rules** — `project_rules.md` 聚合 `.cursor/rules/*.mdc`（原 `CURSOR_RULES.md` 更名）。

---

## [0.2.2] - 2026-05-11

### DS Pick (desktop)

- **v0.2.2** — `deepseek-desktop`、Tauri `version`、`web-ui/package.json` 与侧栏标签对齐 **v0.2.2**；打包脚本与 bundled Python 准备（`prepare-python.mjs`、`docs/bundled-python-plan.md`）。
- **任务与技能** — 侧栏「任务与技能」；`POST /v1/skills` 创建 `SKILL.md` 模板；全局/工作区技能根与桌面文件夹选择器；定时自动化列表 UI 暂缓展示（`fetchAutomations` 保留）。
- 关闭应用时 **自动结束 sidecar**；Web UI 样式 refinements；用户消息气泡可读文本色修复；用量扫描性能、终端主题、**USD 成本** 标签。

### Added

- **办公文档** — `read_file` 支持 `.xlsx` / `.pptx` 文本提取；**`write_office`** 从 JSON 生成 `.xlsx`（Rust）、`.docx` / `.pptx`（Python，主题与 16:9 布局）；详见 [docs/office-doc-capability-plan.md](docs/office-doc-capability-plan.md)。
- **Python 运行时** — `python_env.rs`：`find_python()`、`ensure_office_venv()`（`~/.deepseek/office-py/`）；RLM / `code_execution` 去除硬编码 `python3`。
- **Runtime API** — `POST /v1/skills`；交互式工具审批 HTTP 流（`approval.required` + `resolve-approval`，默认 120s 超时）。

### Fixed

- Shell 工具终端 UX；PPTX 引擎资源扩展。

---

## [0.2.1] - 2026-05-10

### DS Pick (desktop)

- **v0.2.1** — 独立桌面 SemVer；侧栏 **DS Pick v0.2.1**（workspace crate 依赖仍为共享线，如 `0.8.15`）。
- **三栏 shell 增强** — 统一聊天壳、右侧面板（runtime API 对接）、devtools 开关、窗口权限与标题栏。
- **Markdown** — 聊天区 Markdown 渲染；`` `path/to/file` `` 与相对 `[](…)` 链接在工作台打开预览；预览区内相对链接 **应用内** 打开（外链新窗口，`#` 锚点滚动）。
- **启动** — DS Pick 与 sidecar 就绪速度优化。

### Docs

- `docs/desktop/DEV_NOTES.md`；[TUI vs DS Pick 差距表](docs/desktop/TUI_DS_PICK_GAP.md) 初版。

---

## [0.2.0] - 2026-05-09

### DS Pick (desktop)

- **v0.2.0** — DS Pick 产品化里程碑：文档布局、Cursor 规则、桌面与 runtime 一批修复。
- **Tauri 壳** — Sidecar 生命周期、三栏布局、会话文件同步、工作区与附件、运行模式。
- **工作台** — 文件预览模块（文本/图片/二进制）、安全 `read_workspace_binary`；`web_search` Bing 回退。
- **主题** — 明/暗样式与 Markdown 预览 demo 打磨。
- **安全** — CODE_REVIEW：sidecar token 环境变量、CSP、MCP 过滤、updater 相关加固。

### Added

- **Runtime HTTP** — Phase 1 SSE `id` 序列；Phase 2 Tauri + Web MVP；thinking/reasoning 经 SSE 流式输出；工具审批范围与超时配置（Phase 2a）。
- **TUI** — `read_file` / `file_info` 工具链；大文件 plaintext 流式读取；`file_info` RFC3339 `mtime`。

### Fixed

- 会话恢复时还原已保存工作区；sidecar 默认 `cwd` 合理化。

### Docs

- README DS Pick 章节与仓库布局表；桌面文档迁入 `docs/desktop/`。

---

## [0.1.0] - 2026-05-07

### Added

- **Initial fork** — DeepSeek TUI desktop 工作区：共享 `deepseek` CLI/TUI/runtime crates 与 **DS Pick**（`crates/desktop/`）骨架。
- **Runtime API** — `/v1/...` 契约与 [docs/RUNTIME_API.md](docs/RUNTIME_API.md) 实施文档（Phase 1）。

### Changed

- Desktop sidecar / main Rust 源码 `rustfmt`。
