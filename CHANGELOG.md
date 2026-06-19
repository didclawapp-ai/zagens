# Changelog

All notable changes to **Zagens** and its embedded runtime will be documented in this file.

**Update policy:** Record **code- and behavior-related** changes only—typically under `[Unreleased]`, in the **same PR/commit** as the change when practical. Cursor agents: see `.cursor/rules/zagens-repo.mdc` § Changelog.

**Record:** Features, bug fixes, security patches, breaking API/config/runtime behavior, desktop UI/runtime/tool execution changes, and CI/scripts when they change verify, build, or release **semantics**.

**Do not record by default** (transactional / housekeeping): doc moves, translations, README or CONTRIBUTING-only edits, license or repo/org migration, open-source hygiene, screenshot swaps, maintainer runbooks—unless a maintainer **explicitly** asks to include an entry.

**Licensing:** This repository is [MIT](LICENSE). See [NOTICE.md](NOTICE.md) for third-party attribution.

**Zagens** (desktop app in `crates/desktop/`) and the runtime workspace share **`0.8.3`**. Desktop still carries an independent literal in `crates/desktop/Cargo.toml` checked by `check-versions.sh` against Tauri/npm/About. Public releases use `0.MINOR.PATCH` until **1.0.0 GA**. Display form **v** + manifest version (e.g. **v0.8.3**).

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> **2026-05-19:** `[0.3.0]` consolidates work since v0.2.2 (backfilled from `git log`).
> Prefer updating `[Unreleased]` incrementally going forward.

## [Unreleased]

## [0.8.3] - 2026-06-19

**Release highlights**

- **Multi-session parallel streaming (desktop):** Start a turn in session A, switch to B (or create a new session) and send — both streams run live without cancelling each other. Switch back to see progress; background approvals surface a toast; session strip shows spinners and checkmarks per session.
- **Minimal chrome UI:** Icon rail + collapsible session strip, Harness float stack beside the transcript, focus mode (`Mod+.`), streamlined Composer toolbar, and borderless assistant messages.
- **TUI:** Tab / Shift+Tab pane focus restored outside paste bursts.
- **Verify:** `scripts/ci/test-multi-session.{sh,ps1}` and `npm run test:multi-session` for parallel-stream regression.

### Added

- **Desktop runtime proxy (multi-session P0.1):** `runtime_get_sse` and `runtime_cancel_sse` now accept an optional `thread_id`. The SSE cancel map is keyed per `(window, thread)` instead of per-window, so opening a second thread's SSE consumer in the same window no longer cancels the first. `runtime://events-*` payloads are wrapped in `{ thread_id, data }` so the WebView can route concurrent streams to their owners. `thread_id` omitted ⇒ legacy per-window cancel (backwards-compatible with the global Stop path).
- **Desktop web UI (multi-session parallel, P0):** Per-thread `StreamContext` registry; background turns keep running when switching sessions (detach instead of abort); SSE events for non-active threads route into isolated context; reattach on session switch rebuilds transcript and restores composer lock / pending approval; background approval surfaces a persistent toast.
- **Desktop web UI (multi-session, P1):** Session strip shows a pulsing indicator on sessions with in-flight turns; switching back to a streaming session restores checklist/LHT/context panel state from the per-thread registry; stream recovery no longer hijacks `threadTurnRef` for non-active threads; tool deltas rebind to the last assistant bubble after reattach.
- **Desktop web UI (session strip):** Sidebar sessions grouped by date (`yyyy/MM/dd`); up to five rows per day with **More** to expand; completed sessions show a checkmark, in-flight turns show a spinner.
- **Desktop / CI (multi-session verify):** `npm run test:multi-session` (pure helper selfchecks) and `scripts/ci/test-multi-session.{sh,ps1}` (adds `runtime_proxy` unit tests + runtime `parallel_sse_live_streams_filter_by_thread_id` integration test).

### Fixed

- **Desktop web UI (multi-session):** New session composer no longer stays locked while another session streams in the background; navigation detach no longer aborts in-flight SSE before `turn_started` registers the thread; session strip shows a spinning indicator at the title when a session has an active turn; switching away and back preserves the background stream instead of stopping it.
- **Desktop web UI (multi-session):** Background sessions appear in the sidebar after `turn_started` (early persist) and when navigating away from a streaming session (persist + refresh list); streaming checkpoint now covers all in-flight threads, not only the active view.
- **Desktop web UI (multi-session):** Session strip spinner clears when a background turn completes without requiring a click (registry + `streamingThreadIds` stay in sync).
- **TUI (`zagens-tui`):** Restore Tab / Shift+Tab pane focus cycling while the composer is focused; only swallow Tab during active multiline paste bursts (Windows conhost).
- **Desktop web UI:** Clear stuck「生成中」after a turn ends — trust `turn.completed`/`done` for UI unlock, clear streaming on all assistant bubbles (not only the live stream target id), and periodically reconcile when the backend turn is no longer active.
- **Desktop web UI:** Keep Composer inside the chat column so transcript and input stay aligned when Harness float cards are open.
- **Desktop web UI:** Auto-open the right Inspector preview when clicking a workspace file link in chat while the Harness float stack is visible — hide the float stack and expand the panel instead of requiring a manual dismiss.

### Changed

- **Desktop web UI (minimal chrome):** Drop the composer dock top divider for a unified chat canvas; assistant messages render borderless (document flow) while user bubbles keep a light card border.
- **Desktop web UI (minimal chrome, Phase 1–2):** Replace the 240px text sidebar with a 52px icon rail (`IconRail`), collapsible 200px session strip, settings popover menu, and runtime connection dot; add shared chrome primitives (`HarnessCard`, `ProgressScrollViewport`, `RuntimeConnIndicator`) and `progressScroll` layout helper with self-check.
- **Desktop web UI (Harness float stack, Phase 4):** Show four summary cards beside the transcript (`HarnessFloatStack`) with progress viewports; retire the right-side `AuditGridPanel` grid in favor of the in-stage float stack (TitleBar toggle unchanged).
- **Desktop web UI (Composer, Phase 3):** Collapse the second options row into a single toolbar with an overflow (`⋯`) menu for run mode, task type, LHT, auto-approve, and export; paperclip attach icon; accent send button when ready; stronger `focus-within` shell styling.
- **Desktop web UI (Focus mode, Phase 6):** `Mod+.` toggles focus mode — hides session strip, Harness float stack, and right Inspector while preserving underlying panel preferences.
- **Desktop web UI (Inspector, Phase 5):** Workspace panel uses 44px vertical icon tabs (`InspectorIconTabs`) instead of a horizontal text tab bar.
- **Desktop web UI (cleanup, Phase 7):** Remove unused `Sidebar.tsx`, `AuditGridPanel.tsx`, and `useAuditGridData.ts` after the icon-rail / float-stack migration.
- **Audit workflow:** `audit-repo` skill and scratchpad gates now require multi-dimension balance — `kind=cleared` notes need `[D#]` tags and substantive evidence; defer reasons cannot be security-risk-only stubs; report template adds non-security sections (tests, maintainability, architecture).

## [0.8.2] - 2026-06-17

### Changed

- **Runtime (kernel-v2 M4):** `[tools] scheduler` code default is now `dag` (was `shadow`); missing or unknown values resolve to `dag`. `shadow` remains for bake/diff observation; `legacy` remains a kill-switch.

### Fixed

- **TUI onboarding / API key:** First-run key entry and `/api-key` now sync credentials into the live runtime manager and unload the cached thread engine so the next message rebuilds `DeepSeekClient` instead of reusing a pre-onboarding "DeepSeek API key not found" error.

### Added

- **TUI MCP config:** `/mcp` opens an overlay to edit `mcp.json` as JSON (type or paste), with Save/Cancel, validation, and engine reload on the next turn.
- **TUI slash commands:** `/approve` and `/approval` switch the global approval policy (`on-request` / `untrusted` / `never` / `auto`); empty argument cycles like `/lht` and `/theme`, with a picker UI and persistence to `~/.zagens/config.toml`.
- **TUI first-run config:** `zagens-tui` now seeds `~/.zagens/config.toml` from the same `first_run_defaults` template as the desktop app on startup; onboarding API-key save updates that file instead of writing a 4-line legacy stub.

## [0.8.1] - 2026-06-17

### Fixed

- **CLI / TUI:** Missing API key errors and auth recovery hints now reference `zagens login` and `~/.zagens/config.toml` instead of legacy `deepseek auth set` / `~/.deepseek/config.toml`.

## [0.8.0] - 2026-06-17

### Added

- **TUI (`zagens-tui`):** Pending-input preview above the composer while a turn runs; Enter queues during assistant streaming, steers during tool/wait gaps (CodeWhale-style); **Ctrl+Enter** forces steer; **↑** on an empty composer pulls the last queued message back for editing.

- **TUI i18n:** Ratatui chrome now reads `~/.zagens/settings.toml` `locale` (auto / en / ja / zh-Hans / pt-BR) — panels, composer hints, slash picker, help overlay, approval modal, automation overlay, left-rail sessions, and transcript empty state. Added 74 `Tui*` message IDs in `localization.rs` with four-language coverage; `tui/i18n.rs` wires helpers into `AppState.locale`. **`/locale`** and **`/language`** slash commands switch language at runtime (persist settings, refresh UI); empty `/locale` cycles tags.

- **TUI onboarding:** First-run overlay aligned with desktop — welcome, optional DeepSeek API key (Esc skip), default task type (`auto` / `code` / `office`); reads/writes `~/.zagens/settings.toml` (`onboarding_complete`, `task_type_preference`); **`--skip-onboarding`** CLI flag; new threads respect saved task-type preference instead of forcing `code`.

- **TUI slash commands:** **`/api-key`** / **`/key`** save or clear the DeepSeek API key (`clear` removes); **`/login`** / **`/logout`** CLI-aligned aliases.

### Fixed

- **TUI (`zagens-tui`):** Multi-line user prompts show a single `you>` tag with indented continuation lines (paste/send no longer repeats the tag on every row).
- **Desktop (audit 2026-06-17):** Symbol-index IPC (`get_symbol_index_info`, `delete_symbol_index`, `rebuild_symbol_index`) and runtime `POST /v1/symbol-index/rebuild` now require workspace paths under the user home or documents directory; secrets crate docs/`backend_name()` aligned to `~/.zagens/secrets/`; config `redact_secret` fully masks API keys; `openExternalUrl` validates schemes via `URL.protocol`; removed unused `tauri-plugin-shell`; sidecar supervisor log records only `port` from `DS_PICK_READY`; sidecar probe HTTP client no longer panics when TLS stack init fails.
- **TUI (audit PR-A):** ANSI CSI strip recognizes ECMA-48 final bytes (`0x40–0x7E`); composer buffer capped at 128K chars; unfocused transcript border uses transcript style; overlay `centered_rect` deduplicated; expanded thinking/tool detail shows “more lines” ellipsis; draw path adds `trace_span!(tui_draw)`; live-activity waiting state distinguishes edit vs scroll in title/hint and auto-focuses composer when typing to queue.
- **Runtime (Kernel V3 — replay parity):** `verify_step_effect_parity` no longer passes when a step has `ToolCallPlanned` events but no `ModelRequestIssued` anchor in the log.
- **Runtime (Kernel V3 — turn lifecycle):** Streaming `return_early` and pre-inner failure paths now emit `KernelEvent::TurnEnded` before returning from `handle_deepseek_turn`.
- **Runtime (Kernel V3 — capacity replay):** `VerifyWithToolReplay` dispatch returns the live tool-replay outcome instead of discarding it and hardcoding `false`.
- **Runtime (Kernel V3 — approvals):** `RetryWithPolicy` emits `ApprovalVerdict::Retried` instead of mislabeling the decision as `Approved`.
- **Runtime (Kernel V3 — effects):** `Effect::RefreshSystemPrompt` is implemented in `EffectInterpreter` (v3 step + standalone paths); `interpret()` matches all effect variants explicitly.
- **Runtime (Kernel V3 — tool batch):** Tool plan execution polls `cancel_token` between waves and plans; unfilled slots become cancelled outcomes when interrupted.
- **Runtime (Kernel V3 — session resume):** Seeding a new runtime thread on session resume inherits `trust_mode` / `auto_approve` from the linked thread record (request body may override).
- **Runtime (Kernel V3 — steer inject):** `InjectSteer` rejects disallowed control characters and truncates at 8192 chars before transcript injection.
- **Runtime (Kernel V3 — effect interpreter):** v3-step `ExecuteBatch` / `RequestApproval` `NotImplemented` paths now emit structured `warn` logs (missing stream or call_id).
- **Runtime (Kernel V3 — replay counts):** `ReplayEffectCounts` tracks `run_layered_context_checkpoint`, `refresh_system_prompt`, and `emit_artifact`.
- **Runtime (Kernel V3 — capacity):** Capacity checkpoint token fields warn on NaN/overflow clamp; `CapacityAction::from_guardrail` logs non-Continue mappings with guardrail reason at debug level.
- **TUI (composer — Shift+Enter):** Shift+Enter inserts a newline in the prompt; from transcript scroll mode it also focuses the composer so multiline input works without pressing Esc first.
- **TUI (composer — legacy conhost paste):** cmd.exe / conhost injects clipboard text as rapid Char+Enter key events; coalesce same-frame bursts into one multiline insert, extend paste-session detection for slow injection, and route `[` `]` `Tab` `?` (and `j`/`k` during paste) into the composer so sidebars no longer collapse mid-paste.
- **Runtime (LHT — macro CRAFT `on_graph_complete`):** When the checklist/plan graph is complete but micro completion gates are still red, `auto_enter_craft = on_graph_complete` (or `user_confirm`) now evaluates the macro loop **before** unverified/mismatch nudges and can spawn CRAFT or prompt for confirm — previously CRAFT only ran after `graph_complete` (all micro gates green).
- **Runtime (LHT — manifest gate Jest EPERM):** Harness classifies Jest `spawn EPERM` / `spawn EACCES` as infra (not assertion) and appends a `--runInBand` / `.npmrc` cache hint in manifest-failed nudges on Windows-style Node tasks.
- **Runtime (LHT — step-limit continuation regression):** When checklist reached 100% but `update_plan` still had an `InProgress` phase (checklist-driven execution without plan sync), `maybe_continue_at_step_limit` / loop-guard continuations no longer skipped — restores bounded step-budget grants (up to 4× baseline) instead of hard `Reached maximum steps` at 100 tool steps. Abandoned all-pending plans (DEMO5 zombie plan) still do not trigger continuation. `note_incomplete_stop_if_lht` uses the same rule for observability.
- **Desktop (LHT panel):** Long-horizon task timer now ticks until composer **生成中** (`streaming`) ends — no longer freezes when checklist hits 100% while the turn is still running; accumulated time persists across LHT reinject rounds on the same thread.

### Changed

- **Runtime (tool UX — batch A–C):** File tools accept `file` / `file_path` aliases for `path` (TS-01). New **`batch_edit`** and **`refactor_imports`** tools for multi-file search/replace and import-depth remaps (TS-07). Windows Node **workspace preflight** auto-writes `.npmrc` + `jest.config.js` on first user message when `package.json` is present (TS-14). Manifest gate **trusts recent successful test/build exec** instead of re-running identical npm/jest/tsc commands. `exec_shell` records npm/jest/tsc-family successes for verify replay; **tsc/tsconfig** failure hints suggest `refactor_imports` and layout fixes.
- **Runtime (tool UX — batch A):**
  - `read_file` / `write_file` / `edit_file` / `list_dir`: when the model passes `file` / `file_path` / `filename` / `target_path` instead of canonical `path`, return a targeted hint naming the correct field (avoids generic missing-field loops).
  - `edit_file`: reject empty or whitespace-only `search` before apply so `replace_mode: "all"` cannot corrupt the whole file.
- **Runtime (tool UX — batch B):**
  - `exec_shell` / `exec_shell_wait`: on failed npm/jest runs, append `[HINT:…]` blocks for common Windows patterns — npm cache EPERM (workspace `.npmrc`), Jest spawn EPERM (`--runInBand`), npm devDependencies omit (`--include=dev`).
  - Workspace template `fixtures/harness/workspace-templates/nodejs-windows/.npmrc` for local npm cache; `base.md` Windows/Node guidance updated.
- **Runtime (tool UX — batch C):**
  - New `batch_edit` tool: same `search`/`replace` across up to 32 glob-matched files; default `dry_run:true` with per-file diff preview; apply with `dry_run:false` (per-file errors, no rollback of prior writes).
  - New `refactor_imports` tool: remap relative TS/JS/Go imports that resolve to `from_target` → per-file recomputed path to `to_target` (mixed directory depths); same dry-run/limits as `batch_edit`.
- **Runtime (tool UX — batch D):**
  - `checklist_update` / `checklist_write` / `update_plan`: append `[SYNC_WARNING]` when checklist is fully done but matching plan phases stay pending; metadata `plan_checklist_sync_warning`; LHT nudge text aligned with checklist-as-SSOT completion rule.
  - Layer-3 completion gate: optional `[long_horizon.completion_gate.min_lines]` frontend/backend line-count floors (default globs `**/*.{ts,tsx,vue,jsx}` / `**/*.{rs,go,py}`); runs even when deliverable manifest is empty.
- **Runtime (tool UX — batch E partial):**
  - Windows shell: PTY background kill also walks the process tree via `taskkill /T` (C1/T3 complement to existing sync-path tree kill).
  - New `restore_file` tool: restore a single tracked file from git `HEAD` (`git restore`); approval required; use `revert_turn` for full-turn snapshot rollback.

### Removed

- **Runtime (kernel v3 final switch):**
  - Deprecated `TurnLoopHost` trait alias removed; production bounds use `V3TurnHost` only.
  - `KernelMachineMode::Shadow` and `[kernel] machine = "shadow"` shadow bake removed (config still accepted with deprecation warn; behaviour is v3).
  - Runtime shadow observation layer removed (~24 `kernel_*_shadow` modules, global diff stats, `GET /v1/runtime/kernel-shadow`).
  - Turn-end hooks renamed: `finish_kernel_turn`, `kernel_turn_events`, `reset_kernel_turn_events` on `KernelTurnHost`.
  - Production replay verify retained via `kernel_turn_replay_verify`, `kernel_v3_step_verify`, and CI golden fixtures (`fixtures/harness/kernel-v3-replay/`).

### Changed

- **Runtime (kernel v3 Phase D — memory plane compiler alignment):**
  - `QueryMemory` logs `MemoryPlaneQueried`; ContextCompiler force-include/budget overrides derive from log projection (`compiler_queried_sources_from_projection`) instead of a runtime side channel.
  - `verify_memory_plane_compiler_source_coherence` + `verify_compiler_queried_sources_coherence` wired into batch-4 replay gate.
- **Runtime (kernel v3 Phase D — `Effect::EmitArtifact`):**
  - `Effect::EmitArtifact { kind, area_hint }` for scratchpad snapshot/reminder; `ReplayTurnMachine` replays `ScratchpadSummaryInjected` / `ScratchpadReminderInjected` through it (cycle briefing remains `InjectSteer` anchor).
  - v3 live path routes `maybe_inject_scratchpad_*` through `memory_plane_artifact_ops` (no spurious `SteerInjected` on scratchpad inject).
- **Runtime (kernel v3 — core fallback step planning):**
  - `run_v3_step` plans via `LiveTurnMachine::inner_step_live_plan` before streaming/tool phases (non-runtime hosts without `EffectInterpreter`).

### Added

- **Runtime (kernel-v2 Phase 3a — KernelEvent schema + completeness verification):**
  - `crates/core/src/engine/kernel_event.rs` (new): defines `KernelEvent` enum (22 variants, v1 schema). Covers turn lifecycle, model request/delta/message, tool calls, context & compaction, memory injections (steer/scratchpad/cycle), guard decisions (loop-guard/capacity), and LHT continuation events. `#[non_exhaustive]` on all variants; `#[serde(tag = "event_type")]` for stable JSON shape.
  - `RequestFingerprint` and `TurnLoopMode` gain `serde::Serialize`/`Deserialize` derives (required by `KernelEvent` fields).
  - 6 projection functions + 7 completeness verification tests prove all A-class host-state fields (`scratchpad_summary_injected`, `active_tool_names`, `ScratchpadStepState` counters, LHT continuation count, steer injection, capacity state) are rebuildable from the event log — precondition for Phase 3b `TurnMachine::step` purity.
  - `DeferredToolActivated` event variant added (schema-gap fix): captures `maybe_activate_deferred_tool` calls so `active_tool_names` set is fully rebuildable.
  - `ToolCallFinished` gains `tool_name` field for scratchpad-write projection.
  - 4 schema drift ("golden shape") tests + `all_variants_have_kind_str` count guard: CI fails immediately on tag rename, field rename, or silent variant addition.
  - `crates/runtime-adapters/src/persist/kernel_event_log.rs` (new): `KernelEventLog` append-only SQLite writer + `ensure_kernel_events_table` migration. `append_batch` for transaction-coalesced multi-event writes; `load_turn_events` for Phase 3b replay. Tested with in-memory SQLite (4 tests: round-trip, batch, null-turn-id sentinel, idempotent migration).
- **Runtime (kernel-v2 Phase 3b batch 1 — TurnMachine skeleton + double-write wiring):**
  - `crates/core/src/engine/turn_machine.rs` (new): `TurnKernelProjection` (pure projection rebuilt from `KernelEvent` log), `TurnMachine` trait, `Effect` enum, `KernelEventSink` type alias, `emit_kernel` helper. 7 tests cover projection correctness and channel plumbing.
  - `TurnLoopHost::kernel_event_sink()` default method (returns `None`); L2 `host_impl` can override to activate double-write.
  - `run.rs`: double-write at turn start (`TurnStarted`), all turn-end paths (`TurnEnded` with correct outcome), step-limit continuation (`StepLimitContinuation`), loop-guard continuation (`LoopGuardContinuation`), and steer injection (`SteerInjected`).
  - `streaming_phase.rs`: double-write `ModelRequestIssued` (with `RequestFingerprint`) after request assembly; `ModelMessage` (with `Usage`, `block_count`) after session update.
  - `tool_phase.rs`: double-write `ToolCallFinished` (with `tool_name`, `wrote_state`, `outcome`, `duration_ms`) after each tool completes; `DeferredToolActivated` at both deferred-tool activation sites.
  - `RequestFingerprint` and `TurnLoopMode` have `Serialize`/`Deserialize`; all existing tests pass. Total new tests: 25 passing.
- **Runtime (kernel-v2 Phase 3b batch 2 — live double-write + golden replay):**
  - `crates/runtime-adapters/src/persist/kernel_event_writer.rs` (new): `KernelEventWriter` opens `sessions.db`, seeds `SchemaVersion`, spawns async drain task (batched `spawn_blocking` appends). `KernelEventLog::peek_next_seq` / `with_next_seq` for restart-safe sequence numbers.
  - `EngineRuntimeExt::kernel_event_writer` wired in `build_engine`; `TurnLoopHost::kernel_event_sink()` overridden in L2 `host_impl` — turn loop events now persist when sessions dir is available.
  - Golden replay fixtures: `fixtures/harness/kernel-v3-replay/{pure_read,write_batch,lht_continue}.json` + `kernel_event_golden.rs` CI tests (4 tests).
- **Runtime (kernel-v2 Phase 3b batch 2 cont. — projection shadow + EffectInterpreter skeleton):**
  - `LiveTurnSnapshot` + `compare_projection_to_live()` + `ReplayTurnMachine` in `turn_machine.rs`; `emit_kernel_event()` records events for shadow compare alongside SQLite double-write.
  - `kernel_projection_shadow.rs`: per-turn event accumulator; at turn end compares live host snapshot vs log projection; stats exposed via `GET /v1/runtime/kernel-shadow` `projection_shadow` block.
  - `effect_interpreter.rs` skeleton (not yet wired to production turn path).
  - `TurnLoopHost` hooks: `record_kernel_event`, `reset_kernel_projection_shadow`, `finish_kernel_projection_shadow`.
- **Runtime (kernel-v2 Phase 3b batch 3 — kill switch + effect replay shadow):**
  - `[kernel] machine = "legacy" | "shadow" | "v3"` config (`KernelMachineMode`); default `legacy`. Shadow mode runs `ReplayTurnMachine` effect-chain sanity at turn end via `kernel_effect_shadow.rs`; stats on `GET /v1/runtime/kernel-shadow` `effect_shadow` block.
  - `ToolCallPlanned` double-write in `tool_phase.rs` (before execution); golden fixtures updated; `verify_effect_replay_chain()` in `turn_machine.rs`.
  - `ReplayTurnMachine` extended: `ToolCallPlanned` → `ExecuteBatch`, `ToolCallFinished` (state-mutating) → `NotifyLsp`.
  - `EffectInterpreter::ExecuteBatch` returns `DelegatedLegacy` (production path unchanged until `[kernel] machine = "v3"`).
- **Runtime (kernel-v2 Phase 3b batch 2 cont. — v3 seam + ToolCallStarted):**
  - `KernelMachineMode` moved to `zagens-core` (`kernel_mode.rs`); `TurnLoopHost::kernel_machine_mode()` wired in L2 `host_impl`.
  - `turn_loop/v3_driver.rs`: observability when `machine = "v3"` (legacy IO until effect interpreter owns path).
  - `ToolCallStarted` double-write in `tool_phase.rs` (`wave_idx = 0` until DAG wave events land).
  - `EffectInterpreter::interpret_all` batch entry point.
- **Runtime (kernel-v2 Phase 3b batch 3 — v3 IO path via EffectInterpreter):**
  - `[kernel] machine = "v3"`: turn step routes through `engine_v3_step.rs` → `EffectInterpreter::run_call_model_step` / `run_execute_batch_step` instead of inline `run_streaming_phase` / `run_tool_execution_phase` in `run.rs`.
  - `TurnLoopHost::try_run_v3_turn_step` hook (L2 returns `Some`; core fallback for non-runtime hosts).
  - `turn_loop/v3_step.rs`: core fallback orchestration (CallModel → ExecuteBatch ordering + logging).
- **Runtime (kernel-v2 Phase 3b batch 4 — guard ruling event double-write):**
  - `LoopGuardTriggered` from `tool_phase`; `CapacityCheckpoint` from capacity guardrails; `ContextOverflowRecovered` on overflow recovery in `run.rs`.
  - `verify_guard_projection_chain()` + `kernel_guard_shadow.rs`; `GET /v1/runtime/kernel-shadow` adds `guard_shadow`.
  - Golden fixture `fixtures/harness/kernel-v3-replay/loop_guard.json`.
- **Runtime (kernel-v2 Phase 3b batch 5 — Memory Plane event double-write):**
  - `ScratchpadSummaryInjected` / `ScratchpadReminderInjected` from scratchpad inject hooks; `CompactionArtifactCreated` on auto-compaction success; `CycleBriefingInjected` on cycle advance.
  - `TurnLoopHost::sync_kernel_turn_frame` keeps active turn frame for out-of-loop memory events; scratchpad/reminder/compaction hooks take `&TurnContext`.
  - `verify_memory_projection_chain()` + `kernel_memory_shadow.rs`; `GET /v1/runtime/kernel-shadow` adds `memory_shadow`.
  - Golden fixture `fixtures/harness/kernel-v3-replay/scratchpad_compaction.json`.
- **Runtime (kernel-v2 Phase 3b batch 6a — turn replay foundation):**
  - `replay_turn_projection()` + `verify_turn_replay_coherence()` unify projection/effect/guard/memory replay gates (resume substrate).
  - `KernelEventWriter::load_turn_events_sync()` + `verify_persisted_turn_matches()` for SQLite round-trip checks.
  - `kernel_replay_shadow.rs` runs unified coherence at turn end (`[kernel] machine = "shadow"`); `GET /v1/runtime/kernel-shadow` adds `replay_shadow`.
  - Golden test `golden_replay_coherence_all_fixtures` over all kernel-v3-replay JSON fixtures.
- **Runtime (kernel-v2 Phase 3b batch 6b — replay persist + TurnLoopHost seam):**
  - Turn end runs async `finish_kernel_turn_shadow()` (SQLite persist replay after 50ms drain); `KernelEventWriter` held as `Arc` on `EngineRuntimeExt`.
  - `KernelTurnHost` trait extracted from `TurnLoopHost` (kernel double-write / shadow / replay seam).
  - `op_loop.rs` removes `unsafe` dispatch — uses `ext.take()` / restore pattern already documented on `Engine::ext`.
  - `GET /v1/runtime/kernel-replay/{turn_id}` returns projection + coherence for a persisted turn.
  - Session resume reuses linked thread: logs kernel replay coherence for `latest_turn_id` when present.
  - `NOTICE.md`: records engine divergence from CodeWhale upstream from v0.7.x (Kernel v3).
- **Runtime (kernel-v2 Phase 3b batch 6c — thread replay + resume breadth):**
  - `build_thread_replay_report()` aggregates per-turn coherence across a thread's event logs.
  - `GET /v1/runtime/kernel-replay/thread/{thread_id}` returns per-turn replay + thread-level coherence summary.
  - Session resume reuses linked thread: logs kernel replay coherence for **all** persisted turns (not only `latest_turn_id`).
  - Manual `/compact` (`Op::CompactContext`) double-writes `CompactionArtifactCreated` when compaction produces an artifact.
  - Five new golden fixtures (`cycle_handoff`, `overflow_recovery`, `capacity_checkpoint`, `manual_compaction`, `deferred_activation`) — **10** total in `fixtures/harness/kernel-v3-replay/`.
- **Runtime (kernel-v2 Phase 3b batch 6d — thread replay substrate + v3 verify):**
  - `replay_thread_projection()` returns per-turn coherence report plus latest turn [`TurnKernelProjection`] (resume substrate).
  - `ResumeSessionResponse.kernel_replay` populated when reusing a linked thread with kernel events.
  - `[kernel] machine = "v3"` enables unified replay coherence + SQLite persist checks at turn end (effect/guard/memory shadow remains shadow-only).
  - `EffectInterpreter::interpret(RunCompaction)` routes through `run_compaction_effect()` (manual compaction IO).
- **Runtime (kernel-v2 Phase 3b batch 6e — log-driven resume frame + effect replay):**
  - `KernelResumeHints` + `kernel_resume_hints_from_projection()` extract restorable turn frame from latest projection.
  - `Op::ApplyKernelResume` sent after `ensure_engine_loaded` to restore `kernel_active_turn_id` / step frame from persisted kernel log.
  - `replay_turn_effects()` / `replay_effect_counts()` rebuild CallModel/ExecuteBatch chains from event logs (v3/event-interpreter substrate).
  - v3 turn end logs replay effect counts for observability.
- **Runtime (kernel-v2 Phase 3b batch 6f — v3 step effect replay + interpreter consolidation):**
  - `events_for_step()` / `replay_step_effects()` / `verify_step_effect_parity()` slice per-step effect chains from the log.
  - `plan_v3_step_effects()` models v3 `CallModel` + per-tool `ExecuteBatch` plans.
  - `EffectInterpreter::run_v3_turn_step()` consolidates v3 step IO; `kernel_v3_effect_shadow` verifies step parity when `machine = "v3"`.
  - `GET /v1/runtime/kernel-replay/thread/{id}` adds `latest_projection`; session `kernel_replay` adds step/tool summary fields.
  - `GET /v1/runtime/kernel-shadow` adds `v3_effect_shadow` when v3 mode is active.
- **Runtime (kernel-v2 Phase 3b batch 6g — v3 unified step + message replay stats):**
  - `run_v3_turn_step_unified()` (core `v3_step`) collapses runtime interpreter + core fallback; `run.rs` v3 branch is a single call.
  - `ThreadMessageReplayStats` + `replay_thread_message_stats()` aggregate model/tool counters rebuildable from kernel logs (text remains in session store).
  - `verify_session_message_coverage()` logs resume observability when session JSON looks thinner than kernel `ModelMessage` events.
  - Thread replay API + session `kernel_replay` expose model/tool counters; resume path logs coverage diffs.
- **Runtime (kernel-v2 Phase 3b batch 6h — v3 step effect-plan driver):**
  - `EffectInterpreter::run_v3_turn_step()` drives IO via `plan_v3_step_effects()` + `interpret_v3_step_effect()` instead of inline phase calls.
  - Per-tool `ExecuteBatch` plan entries collapse to one runtime batch while preserving replay parity counts.
- **Runtime (kernel-v2 Phase 3b batch 6i — message timeline + structured coverage):**
  - `replay_thread_message_timeline()` rebuilds per-step `ModelMessage` anchors (turn, step, block count) from kernel logs.
  - `SessionMessageCoverage` + `build_session_message_coverage()` return structured session-vs-log diffs (resume + thread replay API).
  - `GET /v1/runtime/kernel-replay/thread/{id}?session_message_count=N` adds `message_timeline` and optional `message_coverage`.
  - `ResumeSessionResponse.kernel_replay` adds `message_coverage_ok` / `message_coverage_summary`.
- **Runtime (kernel-v2 Phase 3b batch 6j — coverage shadow + resume message hints):**
  - `kernel_message_coverage_shadow` records session-vs-log coverage checks at resume / thread replay.
  - `GET /v1/runtime/kernel-shadow` adds `message_coverage_shadow` when `[kernel] machine = "shadow"` or `"v3"`.
  - `KernelResumeHints.kernel_model_message_count` restored from thread projection on engine load.
- **Runtime (kernel-v2 Phase 3b batch 6k — interpret_all v3 driver + timeline shadow):**
  - `EffectInterpreter::interpret_all` routes `CallModel` / `ExecuteBatch` when v3 step context is provided; `run_v3_turn_step` uses plan tail + `interpret_all` instead of a manual effect loop.
  - `verify_message_timeline_coherence` / `verify_message_timeline_vs_session` validate log anchors vs stats and session depth.
  - `kernel_message_timeline_shadow` + `GET /v1/runtime/kernel-shadow` `message_timeline_shadow` when `[kernel] machine = shadow | v3`.
  - `finish_kernel_turn_shadow` moved to `KernelTurnHost` (default on trait); runtime `Engine` override retains full replay pipeline.
- **Runtime (kernel-v2 Phase 3b batch 6l — timeline coverage + KernelTurnHost v3 step):**
  - `SessionMessageTimelineCoverage` + `build_session_message_timeline_coverage()` unify coverage, timeline coherence, and request-count checks.
  - `verify_timeline_vs_request_count()` guards timeline anchors vs `ModelRequestIssued` counts.
  - Thread replay API adds `message_timeline_coverage` when `?session_message_count=N`; resume `kernel_replay` adds `message_timeline_ok` / `message_timeline_summary`.
  - `try_run_v3_turn_step` moved from `TurnLoopHost` to `KernelTurnHost` (`V3ToolRegistry` associated type); `TurnLoopHost` shrinks to runtime IO.
- **Runtime (kernel-v2 Phase 3b batch 6m — message plane index + step anchor shadow):**
  - `ThreadMessagePlaneIndex` + `replay_thread_message_plane_index()` estimate minimum session depth from kernel logs.
  - `verify_session_message_plane_depth()` + `verify_step_model_message_anchor()` strengthen resume/replay without rebuilding message bodies.
  - `build_session_message_timeline_coverage()` adds `plane_depth_ok` / `estimated_min_session_messages`.
  - Thread replay API exposes `message_plane_index`; resume `kernel_replay` adds request count + plane depth hints.
  - v3 step shadow verifies per-step `ModelMessage` anchors alongside effect parity.
- **Runtime (kernel-v2 Phase 3b batch 6n — session role index vs kernel log):**
  - `SessionMessageRoleIndex` + `build_session_message_role_index()` count assistant / tool-result rows in session JSON.
  - `KernelMessageRoleEstimate` + `verify_session_role_index()` compare session role counts to kernel log lower bounds.
  - `build_session_message_timeline_coverage()` adds `role_index_ok` and kernel min assistant/tool-result fields.
  - Session resume builds role index from loaded messages; thread replay API accepts optional `session_assistant_count` / `session_tool_result_count`.
  - `kernel_message_role_shadow` + `GET /v1/runtime/kernel-shadow` `message_role_shadow` when `[kernel] machine = shadow | v3`.
  - Resume `kernel_replay` adds `message_role_index_ok` / `message_role_index_summary`.
- **Runtime (kernel-v2 Phase 3b batch 6o — memory-plane user depth shadow):**
  - `ThreadMessageReplayStats` aggregates scratchpad summary/reminder and cycle briefing counts from kernel logs.
  - `SessionMessageRoleIndex.text_user_message_count` + `KernelMemoryPlaneUserEstimate` weak-check steer/scratchpad injections vs session text-user rows.
  - `build_session_message_timeline_coverage()` adds `memory_plane_user_ok` and kernel min memory-injected user fields.
  - `kernel_message_memory_plane_shadow` + `GET /v1/runtime/kernel-shadow` `message_memory_plane_shadow`.
  - Thread replay API accepts optional `session_text_user_count`; resume `kernel_replay` adds memory-plane hints.
- **Runtime (kernel-v2 Phase 3b batch 6p — compaction depth + continuation user rows):**
  - `ThreadCompactionReplayEntry` + `replay_thread_compaction_timeline()` rebuild compaction `replaced_range` anchors.
  - `verify_session_compaction_depth()` weak-checks current session + removed rows vs kernel plane estimate.
  - Continuation counts in `ThreadMessageReplayStats`; memory-plane user estimate includes step-limit / loop-guard rows.
  - Thread replay API exposes `compaction_timeline` / `compaction_index`; resume adds compaction depth hints.
  - `kernel_message_compaction_shadow` + `GET /v1/runtime/kernel-shadow` `message_compaction_shadow`.

- **Runtime (kernel-v2 Phase 3b batch 6q — compaction artifact cross-check + continuation anchors):**
  - `SessionCompactionArtifactEntry` + `verify_compaction_artifacts_vs_kernel_timeline()` cross-check kernel log vs session-store compaction metadata.
  - `verify_step_continuation_anchor()` ensures continuation steps replay `InjectSteer` effects (v3 event-driven substrate).
  - Session resume loads SQLite compaction artifacts and extends timeline coverage with `compaction_artifact_ok`.
  - `kernel_compaction_artifact_shadow` + resume `kernel_replay` compaction artifact hints; v3 step shadow checks continuation anchors.

- **Runtime (kernel-v2 Phase 3b batch 6r — InjectSteer interpreter + continuation anchor shadow):**
  - `EffectInterpreter` executes `InjectSteer` via `run_inject_steer_effect()` (session transcript + `SteerInjected` kernel event) instead of `DelegatedLegacy`.
  - `verify_thread_continuation_anchors()` thread-level continuation replay check; timeline coverage adds `continuation_anchor_ok`.
  - `kernel_continuation_anchor_shadow` + `GET /v1/runtime/kernel-shadow` `continuation_anchor_shadow`; resume exposes continuation anchor hints.

- **Runtime (kernel-v2 Phase 3b batch 6s — NotifyLsp interpreter + thread replay anchors):**
  - `EffectInterpreter` executes `NotifyLsp` via `flush_pending_lsp_diagnostics()` (v3 step + standalone paths).
  - `verify_step_notify_lsp_anchor()` / `verify_thread_notify_lsp_anchors()` cross-check edit-tool steps vs replay `NotifyLsp` effects.
  - Thread replay API exposes `continuation_anchor_ok` / `notify_lsp_anchor_ok`; timeline coverage includes both anchor fields.
  - `kernel_notify_lsp_anchor_shadow` + `GET /v1/runtime/kernel-shadow` `notify_lsp_anchor_shadow`; v3 step shadow checks notify-LSP anchors.

- **Runtime (kernel-v2 Phase 3b batch 6t — v3 NotifyLsp effect plan tail):**
  - `notify_lsp_effects_from_step_events()` derives post-`ExecuteBatch` `NotifyLsp` effects from `ToolCallFinished` events (matches `ReplayTurnMachine` replay chain).
  - `EffectInterpreter::run_v3_turn_step` appends and executes the notify-LSP tail after tool batch IO, reducing reliance on `run.rs` pre-step flush for edit diagnostics.
  - Core v3 fallback (`v3_step.rs`) runs the same notify tail via `flush_pending_lsp_diagnostics()`; `is_lsp_notify_tool` exported for reuse.

- **Runtime (kernel-v2 Phase 3b batch 6u — continuation event-driven InjectSteer):**
  - `build_step_limit_continue_nudge()` / `build_loop_guard_continue_nudge()` extracted to `long_horizon/nudge.rs`; `CodeTaskGraph::continuation_open_items()` centralizes eligibility.
  - `continuation_ops.rs`: v3 routes step-limit / loop-guard nudges through `EffectInterpreter::InjectSteer` (with nudge body); legacy path unchanged (direct session write).
  - `continuation_inject_steer_effects_for_step()` mirrors replay empty-text `InjectSteer` anchors at step boundaries.

- **Runtime (kernel-v2 Phase 3b batch 6v — memory-plane replay anchors):**
  - `ReplayTurnMachine` maps `ScratchpadReminderInjected` / `ScratchpadSummaryInjected` / `CycleBriefingInjected` → empty-text `InjectSteer` replay anchors.
  - `verify_thread_memory_plane_replay_anchors()` + `memory_plane_inject_steer_effects_from_events()` cross-check memory-plane injections vs replay effect chain.
  - Thread replay / resume expose `memory_plane_replay_anchor_ok`; timeline coverage includes replay anchor field.
  - `kernel_memory_plane_replay_anchor_shadow` + `GET /v1/runtime/kernel-shadow` `memory_plane_replay_anchor_shadow`.

- **Runtime (kernel-v2 Phase 3b batch 6w — compaction replay anchors):**
  - `ReplayTurnMachine` maps `CompactionArtifactCreated` → `RunCompaction` (alongside existing capacity trim/handoff checkpoints).
  - `verify_thread_compaction_replay_anchors()` + `compaction_run_effects_from_events()` cross-check compaction events vs replay effect chain.
  - Thread replay / resume expose `compaction_replay_anchor_ok`; timeline coverage includes replay anchor field.
  - `kernel_compaction_replay_anchor_shadow` + `GET /v1/runtime/kernel-shadow` `compaction_replay_anchor_shadow`.

- **Runtime (kernel-v2 Phase 3b batch 6x — v3 memory-plane InjectSteer + OpenAPI sync):**
  - `memory_plane_ops.rs`: scratchpad summary/reminder injections route through `EffectInterpreter::InjectSteer` when `[kernel] machine = v3`.
  - Regenerated `docs/tech/openapi/zagens-runtime-v1.openapi.json` with resume replay anchor fields (6u–6w).

- **Runtime (kernel-v2 Phase 3b batch 6y — v3 live auto-compaction + step replay anchors):**
  - `compaction_ops.rs`: extract `execute_in_turn_auto_compaction`; v3 routes auto-compaction through `Effect::RunCompaction` (`route_auto_compaction`); manual `/compact` stays on `handle_manual_compaction` (not `RunCompaction`).
  - `verify_step_memory_plane_replay_anchor` / `verify_step_compaction_replay_anchor` + `kernel_v3_effect_shadow` per-step checks.

- **Runtime (kernel-v2 Phase 3b batch 6z — v3 capacity trim/handoff via RunCompaction):**
  - `RunCompactionScope` (`InTurnAuto` / `CapacityTrim` / `CapacityHandoff`) stashed on `EngineRuntimeExt`; `run_compaction_effect` dispatches to the matching IO path.
  - `route_capacity_trim_refresh` / `route_capacity_handoff_replan` wire capacity checkpoints through v3 `RunCompaction` (legacy IO unchanged).

- **Runtime (kernel-v2 Phase 3b batch 6za — v3 cycle advance InjectSteer anchor):**
  - `cycle_briefing_ops.rs`: `route_cycle_advance` routes `perform_cycle_advance` through empty-text `Effect::InjectSteer` when `[kernel] machine = v3` (matches `CycleBriefingInjected` replay anchor).
  - `InjectSteerEffectKind::CycleAdvance` stashed on `EngineRuntimeExt`; `run_inject_steer_effect` dispatches before normal steer text handling.

- **Runtime (kernel-v2 Phase 3b batch 6zb — replay effect counts + anchor-only gate):**
  - `ReplayEffectCounts` aggregates `CallModel` / `ExecuteBatch` / `InjectSteer` / `RunCompaction` / `NotifyLsp` from replay chains; v3 turn-end logging emits all fields.
  - `effect_replay_anchor.rs`: `kernel_effect_replay_anchor_only` suppresses compaction/cycle IO in `run_compaction_effect` and cycle-advance steer dispatch (resume/replay substrate).

- **Runtime (kernel-v2 Phase 3b batch 6zc — replay effect counts observability):**
  - `replay_thread_effect_counts` + `ThreadReplayProjection.effect_counts`; thread/turn replay API expose aggregated counts.
  - `kernel_v3_replay_counts` records last v3 turn counts; `GET /v1/runtime/kernel-shadow` adds `v3_replay_effect_counts`.

- **Runtime (kernel-v2 Phase 3b batch 6zd — resume replay effect counts + OpenAPI):**
  - `ResumeSessionKernelReplay.replay_effect_counts` exposes thread-level replay-chain counts on `POST /v1/sessions/{id}/resume`.
  - `ReplayEffectCounts` gains `JsonSchema`; regenerated `docs/tech/openapi/zagens-runtime-v1.openapi.json`.

- **Runtime (kernel-v2 Phase 3b batch 6ze — resume replay anchor-only interpret):**
  - `apply_kernel_resume_with_replay` loads latest-turn events and re-interprets anchor effects (`RunCompaction` / `InjectSteer` / `NotifyLsp`) under `kernel_effect_replay_anchor_only`.
  - `Op::ApplyKernelResume` routes through replay-aware resume; steer/LSP IO suppressed in anchor-only mode.
  - Regenerated `crates/desktop/web-ui/src/api/generated/runtime-api.ts` (`ReplayEffectCounts` / `replay_effect_counts`).

- **Runtime (kernel-v2 Phase 3b batch 6zf — full-thread resume replay anchor + shadow):**
  - `KernelResumeHints.thread_turn_ids_with_events` carries all turns with kernel events; resume replays anchor effects across the full thread (not latest turn only).
  - `kernel_resume_replay_anchor_shadow` records resume runs / turns interpreted / anchors interpreted / turns skipped; `GET /v1/runtime/kernel-shadow` adds `resume_replay_anchor_shadow`.

- **Runtime (kernel-v2 Phase 3b batch 6zg — resume anchor vs thread replay alignment):**
  - `ReplayEffectCounts::anchor_effect_total` + `KernelResumeHints.expected_anchor_effect_count`; resume compares interpreted anchors vs `replay_thread_effect_counts`.
  - `resume_replay_anchor_shadow` adds `anchor_alignment_checks` / `anchor_alignment_diffs`; `ResumeSessionKernelReplay.replay_anchor_effect_count` on resume API.

- **Runtime (kernel-v2 Phase 3b batch 6zh — RequestApproval effect + replay anchors):**
  - `ReplayTurnMachine` maps `ToolCallPlanned` with `decision.approval_required` → `RequestApproval` before `ExecuteBatch`.
  - `approval_ops.rs`: v3 routes approval handshake through `EffectInterpreter::RequestApproval` (pre-`ExecuteBatch`); legacy `tool_plans_exec` path unchanged.
  - `verify_step_request_approval_anchor()` / `verify_thread_request_approval_anchors()`; thread replay / resume expose `request_approval_anchor_ok`.
  - `kernel_request_approval_anchor_shadow` + `GET /v1/runtime/kernel-shadow` `request_approval_anchor_shadow`; `ReplayEffectCounts.request_approval`.

- **Runtime (kernel-v2 Phase 3b batch 6zi — v3 skip pre-step LSP flush):**
  - `turn_loop/run.rs` skips per-step `flush_pending_lsp_diagnostics()` when `[kernel] machine = v3`; LSP drain is owned by post-`ExecuteBatch` `NotifyLsp` (6t).
  - Legacy / shadow paths unchanged (pre-step flush before `CallModel`).

- **Runtime (kernel-v2 Phase 3b batch 6zj — Sleep effect + capacity cooldown replay):**
  - `CapacityCheckpoint.cooldown_blocked` logged on kernel events; `ReplayTurnMachine` maps blocked `Continue` checkpoints → `Effect::Sleep`.
  - `sleep_ops.rs`: v3 routes cooldown back-off through `EffectInterpreter::Sleep` (`capacity_cooldown_backoff_millis()` symbolic delay).
  - `verify_step_capacity_sleep_anchor()` + v3 step shadow check; `ReplayEffectCounts.sleep`.

- **Runtime (kernel-v2 Phase 3b batch 7a — guard/capacity projection depth):**
  - `TurnKernelProjection` tracks `loop_guard_triggered_count` + `capacity_checkpoint_count`; `guard_projection_policy.rs` pure replay counters.
  - `verify_guard_projection_chain()` cross-checks trigger/continuation/capacity depth vs log (batch 3 substrate).
  - Golden `loop_guard.json` / `capacity_checkpoint.json` guard projection tests.

- **Runtime (kernel-v2 Phase 3b batch 7b — LoopGuard replay simulation):**
  - `loop_guard_replay_policy.rs`: re-simulate `LoopGuard` from `ToolCallPlanned` / `ToolCallFinished` / `LoopGuardContinuation` events.
  - `verify_loop_guard_replay_coherence()` cross-checks `LoopGuardTriggered` anchors vs simulation; wired into `verify_turn_replay_coherence` + `kernel_guard_shadow`.

- **Runtime (kernel-v2 Phase 3b batch 7c — capacity checkpoint replay coherence):**
  - `capacity_replay_policy.rs`: `cooldown_blocked` / `action` field invariants + per-step kind histogram.
  - `verify_capacity_effect_replay_coherence()` aligns `Sleep` / `RunCompaction` replay anchors with checkpoint rows; wired into `verify_turn_replay_coherence` + `kernel_guard_shadow`.

- **Runtime (kernel-v2 Phase 3b batch 7d — v3 capacity checkpoint effect routing):**
  - `capacity_flow/v3_routing.rs`: unified `dispatch_capacity_decision` routes trim/handoff/replay/cooldown through v3 effect plan (`RunCompaction` / `Sleep`) or legacy IO.
  - `run_capacity_*_checkpoint` delegates interventions to dispatcher; post-tool `TargetedContextRefresh` now routes trim; tool replay gains v3 anchor-only gating.

- **Runtime (kernel-v2 Phase 3b batch 8a — Memory Plane layer projection):**
  - `memory_plane_projection_policy.rs`: Working / Episodic (reserved) / Archival layer taxonomy from kernel events.
  - `verify_memory_plane_layer_coherence()` cross-checks layer totals vs `TurnKernelProjection`; wired into `verify_memory_projection_chain` + `kernel_memory_shadow`.
  - Fix `engine.rs` module declarations for `approval_ops`, `sleep_ops`, and `kernel_request_approval_anchor_shadow` (6zh wiring).

- **Runtime (kernel-v2 Phase 3b batch 8b — archival compaction cross-check):**
  - `memory_plane_archival_policy.rs`: archival anchor rebuild, field invariants, and session-store cross-check via `verify_archival_layer_vs_session`.
  - Wired into `verify_memory_projection_chain`; golden `manual_compaction.json` / `scratchpad_compaction.json` archival tests.

- **Runtime (kernel-v2 Phase 3b batch 8c — QueryMemory effect skeleton):**
  - `Effect::QueryMemory { layer, query_key }` + `memory_plane_query_policy.rs` derive pre-`CallModel` reads from projection.
  - `memory_plane_query_ops.rs`: v3 interpreter route with anchor-only short-circuit; `ReplayEffectCounts.query_memory`.

- **Runtime (kernel-v2 Phase 3b batch 8d — Working layer / WorkingSet substrate):**
  - `memory_plane_working_policy.rs`: replay readonly/scratchpad step counters and path-touch substrate from `ToolCallPlanned`/`ToolCallFinished` pairs.
  - `TurnKernelProjection.working_set_path_touch_count` (turn cumulative); `working_set::path_candidates_from_tool_input` helper.
  - `QUERY_WORKING_SET` pre-`CallModel` query when path touches material present; wired into `verify_memory_projection_chain` + golden `pure_read.json`.

- **Runtime (kernel-v2 Phase 3b batch 8e — MemoryPlaneQueried + episodic/compiler wiring):**
  - `KernelEvent::MemoryPlaneQueried` double-write from v3 `QueryMemory` interpreter with `compiler_source` mapping (`memory_plane_compiler_policy.rs`).
  - `memory_plane_episodic_policy.rs`: reserved `QUERY_TOPIC_EPISODIC` + live hints for TopicMemory reads.
  - `memory_plane_query_replay_policy.rs`: query log vs replay/projection coherence checks wired into `verify_turn_replay_coherence`.

- **Runtime (kernel-v2 Phase 3b batch 8f — v3 live QueryMemory before CallModel):**
  - `EffectInterpreter::run_v3_turn_step` runs `plan_v3_pre_call_model_effects` before `CallModel` (projection from kernel shadow + TopicMemory config hints).
  - `query_key_has_projection_material` compiler substrate gate; `verify_step_query_memory_anchor` in v3 effect shadow.

- **Runtime (kernel-v2 Phase 3b batch 8g — TopicMemory kernel event + compiler query wiring):**
  - `KernelEvent::TopicMemoryInjected` double-write from `refresh_system_prompt` when `<topic_memory>` block is composed.
  - Episodic layer replay from `topic_memory_injection_count`; `kernel_memory_query_sources` per-step compiler trace in `compiler_request_context`.

- **Runtime (kernel-v2 Phase 3b batch 8h — Memory Plane batch-4 wrap-up):**
  - Golden fixture `memory_plane_query.json` (WorkingSet + TopicMemory queries + `MemoryPlaneQueried` anchors).
  - `memory_plane_wrapup_policy.rs` batch-4 gate; `kernel_memory_shadow` uses unified coherence check.
  - Compiler force-include + overflow budget overrides for queried `working_set` / `memory.compaction` sources.

- **Runtime (kernel-v2 Phase 3b batch 5a — live steer via InjectSteer effect):**
  - `TurnLoopHost::inject_live_steer`: v3 mode routes `rx_steer` drain through `EffectInterpreter` + `Effect::InjectSteer` (same IO as legacy path via `run_inject_steer_effect`); legacy/shadow unchanged.
  - LHT objective re-inject (`maybe_lht_pre_request_hooks`) uses the same v3 `InjectSteer` path when `[kernel] machine = "v3"`.
  - v3 layered-context checkpoint (`layered_context_checkpoint`) routes through `Effect::RunLayeredContextCheckpoint` + anchor-only replay skip.

- **Runtime (kernel-v2 Phase 3b batch 5a cont. — layered context seam kernel event):**
  - `KernelEvent::LayeredContextSeamInjected` double-write from successful Flash seam append; `ReplayTurnMachine` derives `RunLayeredContextCheckpoint`.
  - `layered_context_replay_policy.rs` + golden fixture `layered_context_seam.json`.

  - Closes batch-5 pre-step gaps called out in `AGENT_KERNEL_V3_PHASE3_DESIGN.md` §6.2 migration table.

- **Runtime (kernel-v2 Phase 3b batch 5c — resume log/session parity golden):**
  - `kernel_resume_parity_policy.rs`: `verify_thread_resume_log_session_parity` (thread replay coherence + anchor alignment + `build_session_message_timeline_coverage`).
  - Golden fixture `fixtures/harness/kernel-v3-replay/resume_thread_parity.json` + CI test `golden_resume_log_session_parity_fixtures` (counter/timeline level; transcript bytes remain a closure follow-up).

- **Runtime (kernel-v2 Phase 3b batch 5c cont. — message body preview + transcript rebuild):**
  - `KernelEvent::ModelMessage.text_preview` and `ToolCallFinished.result_preview` double-write from streaming/tool phases.
  - `message_body_rebuild_policy.rs`: `rebuild_transcript_from_events` / `verify_log_transcript_rebuild` (preview-level transcript rows).
  - Golden fixture `message_body_rebuild.json` + `golden_message_body_rebuild_fixture`; `resume_thread_parity.json` extended with preview fields + transcript check in `golden_resume_log_session_parity_fixtures`.

- **Runtime (kernel-v2 Phase 3b batch 5c cont. — transcript preview index + resume/API gate):**
  - `ThreadTranscriptPreviewIndex` + `replay_thread_transcript_preview_index` wired into `replay_thread_projection` and `build_session_message_timeline_coverage` (`transcript_preview_ok` when preview bodies exist).
  - `KernelResumeHints` carries preview row/body counts; resume session + thread replay API expose `kernel_transcript_preview_row_count` / `message_transcript_preview_ok`.

- **Runtime (kernel-v2 Phase 3b batch 5c cont. — preview body parity vs session):**
  - `rebuild_preview_messages_from_thread_events` + `verify_session_transcript_preview_bodies` (role + preview text).
  - `build_session_message_timeline_coverage` accepts optional session rows; `transcript_preview_body_ok` in resume/kernel-replay responses.

- **Runtime (kernel-v2 Phase 3b batch 5c cont. — v3 log transcript repair on resume):**
  - `[kernel] log_transcript_repair = true` (default off; requires `machine = "v3"`) replaces divergent engine session rows with log-rebuilt preview transcript on `ApplyKernelResume` when preview bodies exist.
  - `kernel_log_session_repair.rs` + shadow counters; `should_repair_session_from_kernel_log` policy in core.
  - `[kernel] log_transcript_repair_persist = true` (default off; requires `log_transcript_repair`) writes repaired preview rows to `~/.deepseek/sessions` via `runtime_thread_id` lookup; HTTP sidecar injects `SessionManager` into engines.

- **Runtime (kernel-v2 Phase 3b batch 5c closure — full session JSON byte parity):**
  - `KernelEvent::ModelMessage.assistant_text` and `ToolCallFinished.session_content` double-write from streaming/tool phases (exact session bodies; previews remain for legacy logs).
  - `message_body_rebuild_policy.rs`: `rebuild_session_messages_from_events`, `verify_session_messages_byte_parity`, `verify_session_messages_structural_parity`; preview rebuild delegates to full rebuild when closure fields exist.
  - Golden session fixtures `message_body_rebuild.session.json` / `resume_thread_parity.session.json` + `golden_session_messages_byte_parity_fixtures`; resume parity golden extended with byte check.
  - v3 log transcript repair (`kernel_log_session_repair.rs`) now applies full session rebuild instead of preview-only rows.

- **Runtime (kernel-v2 Phase 3b batch 5d — default v3 turn machine):**
  - `[kernel] machine` default is now `v3` (`KernelMachineMode::parse(None)` / `Default`); explicit `legacy` and `shadow` remain kill switches; unknown config values still resolve to `legacy`.
  - `TurnLoopHost` trait surface baseline test tracks **60** methods during host strangler migration.

- **Runtime (kernel-v2 Phase 3b batch 5d cont. — TurnLoopHost strangler split):**
  - `TurnLoopHost` composes `TurnLoopSessionHost` (16) + `LegacyInnerStepHost` (26) + `TurnLoopOuterHost` (18); streaming/tool phases bound on `LegacyInnerStepHost` only.
  - Baseline inventory tests per seam; delete `LegacyInnerStepHost` when `[kernel] machine = legacy` kill switch is removed.

- **Runtime (kernel-v2 Phase 3b batch 5d cont. — outer loop policy + legacy path isolation):**
  - `live_outer_loop_policy.rs`: pure step-limit / overflow / loop-guard / cycle-advance decisions + `inner_step_io_path` (v3 vs legacy kill switch).
  - `legacy_inner_step.rs`: legacy/shadow streaming+tool path extracted from `run.rs`; default v3 uses `run_v3_turn_step_unified` only.

- **Runtime (kernel-v2 Phase 3b batch 5b cont. — live outer-loop driver):**
  - `live_turn_outer_driver.rs`: gate enums + `OuterBoundaryGrant` records (counter/event/status updates after host confirms LHT continuation); aligned with `ReplayTurnMachine` continuation effects.
  - `run.rs` delegates step-limit / overflow / loop-guard / in-turn cycle grants to the driver; host IO remains on `TurnLoopOuterHost` until `TurnMachine::step` absorbs outer decisions.

- **Runtime (kernel-v2 Phase 3b batch 5b cont. — pre-inner baseline via EffectInterpreter):**
  - `KernelTurnHost::try_run_pre_inner_step_baseline` + `run_pre_inner_step_baseline` in core driver; v3 runs `plan_v3_pre_inner_step_baseline()` through `EffectInterpreter` (`RunCompaction` slot 0 + `RunLayeredContextCheckpoint` slot 1).
  - `pre_inner_step_ops.rs`: unified `run_v3_pre_inner_step_baseline`; `run.rs` single entry replaces separate host compaction/layered calls.

- **Runtime (kernel-v2 Phase 3b batch 5b cont. — capacity hold driver):**
  - `live_turn_outer_driver`: `post_inner_error_escalation_gate`, `run_capacity_pre_request_hold`, `run_capacity_error_escalation_hold` (policy gate + checkpoint IO + v3 hold boundary logging).
  - `run.rs` routes pre-request and error-escalation capacity holds through the driver; v3 trim/handoff IO remains in `capacity_flow/v3_routing` via `EffectInterpreter`.

- **Runtime (kernel-v2 Phase 3b batch 5b cont. — LiveTurnMachine drives `run.rs`):**
  - `live_turn_machine.rs`: `LiveOuterLoopState` (continuation counters + per-turn scratch) and `LiveTurnMachine` (`TurnMachine` planning facade delegating replay to `ReplayTurnMachine`).
  - `run.rs` outer-loop gates/grants and `apply_outer_boundary_grant` routed through `LiveTurnMachine`; `end_turn` reads `LiveOuterLoopState` snapshot.

- **Runtime (kernel-v2 Phase 3b batch 5d cont. — outer-loop host seam):**
  - `OuterLoopHost` (`TurnLoopOuterHost` + `KernelTurnHost`) bounds outer-loop IO helpers (`apply_outer_boundary_grant`, capacity holds, pre-inner baseline, `end_turn`); `handle_deepseek_turn` still requires full `TurnLoopHost` for inner-step IO.
  - `v3_driver` logging tightened to `KernelTurnHost` / `OuterLoopHost` instead of monolith trait.

- **Runtime (kernel-v2 Phase 3b batch 5d cont. — shadow routes v3 turn loop):**
  - `KernelMachineMode::uses_v3_turn_loop()` now includes `shadow`; `uses_legacy_turn_loop()` is `legacy` only.
  - `legacy_inner_step` / direct streaming+tool phases are the sole legacy kill switch; shadow bake runs v3 IO plus effect/guard/memory shadow checks.

- **Runtime (kernel-v2 Phase 3b batch 5b cont. — outer boundary grant replay coherence):**
  - `verify_outer_boundary_grant_replay_coherence` + `LiveTurnMachine::verify_boundary_grant` validate grant records against `ReplayTurnMachine` effect plans at apply time.
  - `apply_outer_boundary_grant` logs replay plan (debug) and warns on v3 coherence diffs.

- **Runtime (kernel-v2 Phase 3b batch 5b cont. — LiveTurnMachine outer pre-inner segment):**
  - `plan_v3_outer_pre_inner_step_effects` documents host refresh seam + baseline effect slots; `run_outer_pre_inner_step_via_machine` drives refresh/LHT/baseline/capacity/overflow before inner step.
  - `run.rs` delegates outer pre-inner IO to `LiveTurnMachine`; `OuterPreInnerStepOutcome` controls continue/break/fail/proceed.
  - `drain_live_steers_via_machine`: `rx_steer` batch → `InjectSteer` (v3 via `EffectInterpreter`); `plan_live_steer_inject_effects` + v3 drain logging.
  - `system_prompt_refresh_policy`: `QueryMemory` plan for user memory + topic episodic; `refresh_system_prompt_via_machine` logs v3 plan then runs host IO (compiler effect migration pending).
  - `system_prompt_refresh_ops.rs`: v3 runs planned `QueryMemory` chain via `EffectInterpreter` before host `refresh_system_prompt`; `KernelTurnHost::try_run_system_prompt_refresh_queries`.
  - `try_run_system_prompt_refresh` consolidates QueryMemory + assembly in runtime ops; v3 skips direct `TurnLoopOuterHost::refresh_system_prompt`.
  - `Effect::RefreshSystemPrompt` + `plan_system_prompt_refresh_effects` (QueryMemory ×2 + assembly tail); `host_io_required = false` on v3 plan.
  - `system_prompt_refresh_replay_policy`: `ReplayTurnMachine` emits `RefreshSystemPrompt` when refresh `topic_episodic` follows `user_memory` at the same step; wired into `verify_turn_replay_coherence`.
  - Unknown `[kernel] machine` config values now default to `v3` (was `legacy`).
  - `log_legacy_turn_loop_deprecation` warns when `[kernel] machine = legacy` kill switch is active.

- **Fix (desktop/sidecar — op-loop dispatch regression):**
  - Restore disjoint-field platform dispatch in `op_loop.rs` (reverts Phase 3a `ext.take()` regression): `EngineRuntimeExt` stays in `Engine::ext` during `dispatch_op`, so `runtime_ext_mut()` works on the turn/resume path; fixes sidecar panic `tui builder stores EngineRuntimeExt in Engine::ext` and desktop turns with no model response.
- **Fix (TUI composer — multiline paste):**
  - Enable bracketed paste in the terminal so clipboard text arrives as `Event::Paste`; accept `\n` in typed char fallback; `Ctrl+V` / `Shift+Insert` paste when composer focused.
  - `ComposerPasteGuard` treats rapid Enter-after-char bursts as in-composer newlines when the terminal ignores bracketed paste (Windows Terminal “仍然粘贴” path); input events are batched per frame so paste streams coalesce before send.
- **Fix (runtime thread events — SQLite seq allocation):**
  - `append_event_sqlite` allocates `events.seq` inside a DB transaction (`COALESCE(MAX(seq),0)+1`) instead of trusting each process's in-memory counter; fixes `UNIQUE constraint failed: events.seq` when TUI and sidecar share `runtime.db`, and migration now seeds `next_seq` from migrated events rather than stale `state.json`.

- **Runtime (kernel-v2 Phase 3b batch 5 closure — legacy turn loop removed):**
  - `KernelMachineMode::Legacy` deleted; `[kernel] machine = "legacy"` maps to v3 with startup warn. Inner step always routes through `run_inner_step_via_machine` / `EffectInterpreter`.
  - Removed `legacy_inner_step.rs`, `InnerStepIoPath`, and pre-inner LSP flush (v3 owns `NotifyLsp` tail).
  - `system_prompt_refresh.json` golden fixture + replay coherence in `verify_turn_replay_coherence`.
  - Session log-first (resume): `[kernel] log_transcript_repair` defaults to `true` (persist still opt-in via `log_transcript_repair_persist`).
  - `ReplayTurnMachine`: log-order-aware `QueryMemory` replay (pre-`ModelRequestIssued` from `MemoryPlaneQueried`; post-request from projection with key dedup).
  - `LegacyInnerStepHost` renamed to `InnerStepHost` (`inner_step_host.rs`); legacy kill switch removed.
  - `live_turn_inner_driver`: inner-step live IO follows `LiveTurnMachine::inner_step_live_plan` through `EffectInterpreter` (replaces inline `plan_v3_*` sequencing in runtime).
  - `V3TurnHost` replaces monolithic `TurnLoopHost` for turn-loop bounds; `TurnLoopHost` retained as deprecated adapter shim only.

- **Runtime (kernel-v2 Phase 3b batch 5b cont. — LiveTurnMachine outer step-frame segment):**
  - `plan_v3_outer_step_frame_effects`: documents scratchpad reset + kernel turn-frame sync + cancel gate before pre-inner work.
  - `run_outer_step_frame_via_machine` drives per-iteration frame IO; `run.rs` delegates reset/sync/cancel to `LiveTurnMachine`.

- **Runtime (kernel-v2 Phase 3b batch 5b cont. — LiveTurnMachine inner step segment):**
  - `live_turn_inner_planner`: `InnerStepEffectPlan` + `plan_v3_inner_step_baseline` (`QueryMemory` → `CallModel` → dynamic `ExecuteBatch` / `NotifyLsp` tail).
  - `run_inner_step_via_machine` logs baseline plan, verifies `TurnMachine::step` coherence on `ModelRequestIssued`, then runs `EffectInterpreter` IO.
  - `run.rs` v3 inner step delegates to `LiveTurnMachine` (legacy path unchanged).
  - `inner_step_replay_policy`: post-IO `ModelRequestIssued` from turn log drives `ReplayTurnMachine::step` (replaces synthetic pre-IO baseline verify).
  - `replay_step_effects` / `replay_step_effects_from_turn_log`: prefix-projection-aware step replay (multi-step safe); post-IO slice parity via `verify_inner_step_slice_replay_coherence`.

- **Runtime (kernel-v2 Phase 3b batch 5b cont. — LiveTurnMachine outer post-inner segment):**
  - `plan_v3_outer_post_inner_step_effects` documents conditional loop-guard / error-escalation hold / in-turn cycle advance slots.
  - `run_outer_post_inner_step_via_machine` drives post-tool loop-guard, capacity escalation hold, scratchpad reminder, and cycle advance; `OuterPostInnerStepOutcome` controls continue/break/advance.

- **Runtime (kernel-v2 Phase 3b batch 5b — outer continuation boundary policy):**
  - `continuation_boundary_policy.rs` centralizes step-limit / loop-guard grant eligibility and budget math from `run.rs`.
  - v3 logs `kernel_v3` continuation boundary grants aligned with `StepLimitContinuation` / `LoopGuardContinuation` events.
  - Extended with context-overflow cycle handoff + in-turn cycle advance eligibility, hard-fail message, and overflow strategy helpers.
  - `live_turn_outer_planner.rs` documents v3 pre-inner-step baseline (`RunCompaction` + `RunLayeredContextCheckpoint`) and capacity checkpoint effect tails; pre-request / error-escalation capacity holds log under `kernel_v3`.
  - `kernel_capacity_tail_shadow.rs` compares planned vs interpreted capacity tails on v3 dispatch; exposed via `GET /v1/runtime/kernel-shadow` (`capacity_tail_shadow`).
  - v3 pre-inner-step baseline driven by `TurnLoopHost::run_pre_inner_step_*` + `pre_inner_step_ops.rs` (`EffectInterpreter` for `RunCompaction` / `RunLayeredContextCheckpoint`).
  - `kernel_pre_inner_step_baseline_shadow.rs` tracks baseline step / slot interpreter counts; exposed via `GET /v1/runtime/kernel-shadow` (`pre_inner_step_baseline_shadow`).
  - `outer_boundary_replay_policy.rs` + `kernel_outer_boundary_shadow.rs`: outer-boundary cap replay + v3 grant/event alignment; exposed via `kernel-shadow` (`outer_boundary_shadow`).
  - v3 step-limit / loop-guard continuations routed via `outer_boundary_ops.rs` (replay-aligned empty `InjectSteer` + pending IO), matching `ReplayTurnMachine`.
  - v3 overflow / in-turn cycle handoffs routed via `outer_boundary_ops` + `CycleAdvanced` kernel event on in-turn advance; grant shadow re-enabled for `InTurnCycleAdvance`.
  - v3 capacity hold boundaries (`PreRequestCapacityHold` / `ErrorEscalationCapacityHold`) log planned vs interpreted capacity tails via `capacity_hold_ops` + `plan_capacity_hold_boundary_effect` (aligned with `EffectInterpreter` dispatch).
  - **5c cont.:** `LiveTurnSnapshot.in_turn_cycle_advances` + log projection split (`CycleAdvanced` vs overflow `ContextOverflowRecovered`); golden `golden_live_projection_cycle_counter_split`.
  - **5c cont.:** `KernelResumeHints` carries outer-loop continuation counters; `verify_thread_resume_projection_counter_parity` + golden `golden_resume_projection_counter_parity_fixtures`.

### Removed

- **Runtime (kernel-v2 Phase 2 legacy cleanup — G-PR):** Legacy and Shadow context injection paths removed; `ContextCompiler` V2 is now the sole request-assembly path:
  - `ContextCompilerMode::Legacy` and `ContextCompilerMode::Shadow` variants deleted from the enum. `"legacy"` and `"shadow"` config values still accepted in `config.toml` (silently mapped to V2 for parse compatibility) but have no behavioural effect.
  - `streaming_phase`: removed `.or_else(|| session.system_prompt.clone())` legacy fallback — system prompt now comes exclusively from the compiler snapshot.
  - `context_recovery::try_budget_recompile`: removed `ContextCompilerMode::Legacy` kill-switch guard; budget solver always active.
  - `host_impl::model_request_fingerprint`: removed Shadow comparison block (`shadow_compare_with_snapshot` call).
  - `host_impl::compiler_request_context`: removed `!= V2` guard; always runs V2 path.
  - `context_compiler_shadow`: removed global shadow atomics (`SHADOW_COMPARISONS`, `SHADOW_STATIC_DIFFS`, `SHADOW_FULL_DIFFS`), `ContextCompilerShadowStats`, `context_compiler_shadow_stats()`, `record_context_compiler_shadow_diff()`, `shadow_compare()`, `shadow_compare_with_snapshot()`, `compute_compiler_fingerprint_from_snapshot()`, `SessionProxy`, `compiler_source_layer()`, and related tests. Module renamed in purpose to V2 state-snapshot + source-graph construction only.
  - `GET /v1/runtime/kernel-shadow`: `context_compiler_shadow` field removed from response (was `None` in V2 mode anyway).
  - `diagnostics` tool: `context_compiler_shadow` section removed.

### Changed

- **Runtime (kernel-v2 G-PR — M3 policy default + Phase 2 compiler default):**
  - `ToolsPolicyMode` 代码默认值从 `Legacy` 改为 `Engine`（对应 `[tools] policy`）；parse fallback 同步更改。`Legacy` 变体保留为 kill-switch（`policy = "legacy"`）。行为变更：`PolicyEngine` 现在开箱即用地控制审批、并行度和沙箱决策——无需 config 配置。
  - `ContextCompilerMode` 代码默认值从 `Legacy` 改为 `Shadow`（对应 `[context] compiler`）；parse fallback 同步更改。`Legacy` 变体保留为 kill-switch（`compiler = "legacy"`）。行为变更：`ContextCompiler` shadow 模式默认激活——所有用户无需 config 条目即可在 `GET /v1/runtime/kernel-shadow` 累积 `context_compiler_shadow` 统计。

- **Runtime (kernel-v2 P2-Switch — policy bridge G-PR + Phase 2 V2 default):**
  - `policy_bridge`：删除 `legacy_tool_plan_approval_meta` 公开导出；提取独立的 `build_approval_description` 私有辅助函数（description 来源与 policy 决策解耦）。`Shadow` 模式 bake 周期结束，现在映射为 `Engine` 行为（不再记录 legacy 比对，shadow 计数器停止增长）。`GET /v1/runtime/kernel-shadow` 的 `policy_shadow` 字段仅在显式设置 `policy = "shadow"` 时出现。
  - `ContextCompilerMode` 代码默认值从 `Shadow` 改为 `V2`（对应 `[context] compiler`）；parse fallback 同步更改。Phase 2 P2-Switch：`TurnLoopHost` 新增 `context_compiler_system_prompt()` 钩子；L2 实现在 V2 模式下通过 `assemble_system_text_for_v2` 从 `ContextCompiler` 源图组装 system prompt，`streaming_phase` 优先使用此钩子结果（Legacy/Shadow 下退回 `session.system_prompt`）。V2 模式下 system text 与 legacy 路径字节完全一致（0-diff，shadow bake 验证）。`GET /v1/runtime/kernel-shadow` 的 `context_compiler_shadow` 字段仅在显式设置 `compiler = "shadow"` 时出现。

- **Runtime (kernel-v2 M4 — DAG scheduler bake):**
  - `ToolsSchedulerMode` 代码默认值从 `Legacy` 改为 `Shadow`（对应 `[tools] scheduler`）；parse fallback 同步更改。`Legacy` 变体保留为 kill-switch（`scheduler = "legacy"`）。行为变更：DAG 调度器 shadow 模式默认激活——`resolve_execution_groups` 在每次工具批次调度时并发运行 DAG 波次计算并记录与 legacy 批次的 diff，但 legacy 顺序仍控制实际执行。`GET /v1/runtime/kernel-shadow` 现在在默认 config 下始终返回 `scheduler_shadow` 字段（M4 bake 计数器可实时观察）。目标 diff 率 < N% 后翻转为 `dag`（待观察后决定阈值）。

- **Runtime (kernel-v2 Phase 2 遗留补全 — budget solver wire-up + scratchpad.reminder 预算估算):**
  - `try_budget_recompile`（`context_recovery.rs`）从 stub 升级为真实实现：构建 `ContextCompilerStateSnapshot` + `build_compiler_from_snapshot`，计算 source-only 预算（`total_budget - message_tokens`），调用 `compile_with_budget_override`；当 `overflow_recovered = true` 时设置 `overflow_source_budget_cap`（core Engine 新增字段）并直接返回 `true`——跳过 LLM 强制压缩步骤。Budget solver 首次真正接入 overflow recovery 路径（P2-D 完成）。
  - `compiler_request_context` 消费 `overflow_source_budget_cap`（per-retry，`.take()` 立即清零），调用 `compile_with_budget_override(cap)` 代替 `compile()`，并根据 `contributions` 逐源决定是否包含 `memory.compaction` 文本（system_prompt 组装）和 `working_set` 文本（turn_meta_text）。Eviction-aware 请求组装完成。
  - `TurnLoopHost::compiler_request_context` 签名从 `&self` 改为 `&mut self`（以干净地消费 overflow_source_budget_cap，无需 unsafe）。
  - `ContextCompilerStateSnapshot` 新增 `scratchpad_reminder_est_tokens: u32`（pure-logic，无 I/O）；`compiler_request_context` 通过新 helper `scratchpad_reminder_est_tokens()` 填充——当 `readonly_tool_successes ≥ remind_after_readonly_tools` 且无写入时返回固定常量 `SCRATCHPAD_REMINDER_TOKEN_ESTIMATE = 80`（budget accounting 占位）。`scratchpad.reminder` source render 从空升级为 `RenderedBlock::placeholder(n)` 以给 budget solver 提供准确的 Volatile token 账目。

- **Runtime (kernel-v2 Phase 2 missing sources — tools.catalog、scratchpad.reminder、steer):**
  - `ContextCompilerStateSnapshot` 新增 `tool_catalog_est_tokens: u32`（默认 12000），作为 `tools.catalog` 的 StaticPrefix 预算占位符；`TurnLoopHost::compiler_request_context(active_tools)` 替换旧的 `context_compiler_system_prompt`，返回 `CompilerRequestContext { system_prompt, turn_meta_text }` 聚合体——单次快照同时驱动 system prompt 组装和消息 `<turn_meta>` 注入，用真实序列化 JSON 的 token 估算覆盖默认值。
  - `build_compiler_from_snapshot` 由 4 个 source 扩展至 7 个：新增 `tools.catalog`（StaticPrefix，priority 254，Fixed(12000)，render 返回 `RenderedBlock::placeholder`）、`scratchpad.reminder`（Volatile，priority 140，Elastic{0,800}，render 空）、`steer`（Volatile，priority 100，Elastic{0,2000}，render 空）。编译器 source 图完整。
  - `RenderedBlock::placeholder(token_count)` 新 API：空文本但携带指定 token 数，供 budget solver 保留预算——`compile` 和 `compile_with_budget_override` 对空文本块使用存储的 `token_count` 而非 `estimate_text()`。
  - `messages_with_turn_metadata_compiled(session, workspace, turn_meta_override)` 新函数：V2 mode 使用 compiler snapshot 的 `working_set_text` 替换 session 直接计算——turn_meta 文本来源从 session.working_set 切换至编译器快照（字节级等价）。`streaming_phase` 由单字段 `system_prompt` 升级为 `CompilerRequestContext` 聚合体，一次 snapshot 完整驱动请求。

### Added

- **M4 shadow bake tooling fix + 首次 shadow 观测（2026-06-15）:** `scripts/kernel-v2-corpus-run.ps1` was silently skipping `GET /v1/runtime/kernel-shadow` probing when `-ToolsScheduler` was not explicitly passed, even though the runtime default is `shadow`. Fixed: shadow stats are now probed after every scenario run unless the scheduler is explicitly set to `legacy` or `dag`. `kernel_v2_shadow_bake_report.py` extended with per-`batch_shape` breakdown and a dedicated M4 write-shape safety gate (`write` diffs must be 0 to clear the flip-to-dag decision). **首次 shadow 模式语料库观测**（`m4-shadow-bake-shadow-mode`，10 场景）：`pure_read` 0 diffs（5 次比较），`shell_degradable` 3 diffs（30%，预期：DAG 识别只读 shell 可并行），`write` 2 diffs（9.1%）。Write diff 根因：均为 DAG 在同一模型响应的多工具批次中正确并行化无资源冲突的操作对（`tool_search + todo_write`），非写竞态——"零写竞态告警"gate 实质通过，进入翻转决策窗口。

- **CRAFT (C10 — multi-model routing, all spawn paths):** Sub-agent role-specific model overrides now apply to every spawn pathway:
  - Config keys `[subagents] verifier_model` / `review_model` / `implementer_model` / `auditor_model` / `explorer_model` / `default_model` were already wired through the manual `agent_spawn` tool path (`configured_model_for_role_or_type`).
  - **Fix:** `executor.rs` C1 fix-loop auto-spawn and C8 pre-review gate auto-spawn were using the parent model unconditionally, bypassing `role_models`. Added `SubAgentRuntime::role_model_override(agent_type)` (lookup order: type key → `"default"` → `None`) and applied it to both executor spawn sites via `SubAgentSpawnOptions::model`.
  - Net result: `implementer_model` (and `default_model` fallback) is now honoured for all programmatic Implementer re-spawns, not just user-initiated `agent_spawn` calls.

- **CRAFT (C5 — capability attenuation, Review + Verifier):** Sub-agent tool surfaces are now role-scoped:
  - `SubAgentType::Review`: `exec_shell` removed from read-only cap. Reviewer is truly read-only (list_dir, read_file, grep_files, glob_files, file_search, note). It annotates `[verify: cmd]` tags but does not execute them — consistent with C3 evidence-gate design where execution is the Verifier's job.
  - `SubAgentType::Verifier`: explicit cap added (`verifier_tool_cap()`): read tools + `exec_shell` + `run_tests` + `diagnostics` + `note`. No write tools (`write_file`/`edit_file`/`apply_patch`), no `agent_spawn`. Verifier runs verify commands; it does not modify source or spawn further agents.
  - `build_allowed_tools` explicit-tools path: now intersects caller-provided lists with the role's hard cap for `Review` and `Verifier`, preventing bypass via `allowed_tools`.
  - 4 new unit tests (`build_allowed_tools_review_*`, `build_allowed_tools_verifier_*`), all pass.

- **CRAFT (C11 — traceability matrix, minimal form):** `[req: ID]` tag mechanism added as structural complement to `[verify: cmd]`:
  - `long_horizon/verify.rs`: `parse_req_tag()` / `parse_all_req_tags()` / `strip_req_tags()` — parse `[req: ID]` tags from checklist item content (deduplicated, order-preserving). 3 unit tests.
  - `tools/subagent/blackboard.rs`: `RequirementEntry` struct; `write_task_requirements()` writes a requirements list to the blackboard `requirements` partition; `read_task_requirements()` reads it back; `check_requirement_coverage()` cross-references requirements against checklist items carrying `[req: ID]` tags and returns `RequirementCoverage` entries marking each requirement as covered or orphaned; `format_requirements_for_reviewer()` formats a Markdown coverage report (✅ covered / ⚠️ orphaned). 4 unit tests.
  - Reviewer `read_blackboard_section` injection: when requirements are registered for the task, the Reviewer prompt now includes a "Requirements to verify against" section so the reviewer can explicitly check coverage. Opt-in / backwards-compatible — existing CRAFT workflows without requirements see no change.
  - Full parallel-generation landing (P0.5/P1.5 two-gate pipeline per `PARALLEL_FRESH_GENERATION.md`) deferred to 0.8 + de-risk experiments.
  - `long_horizon/mod.rs`: `parse_all_req_tags` re-exported as `pub(crate)`.

- **Harness (LHT/CRAFT batch-A product track):** Three independent improvements shipped without Kernel V2 dependency:
  - **L1 doc alignment:** `LONG_HORIZON_CODE_TASKS.md` and `COMPOSABLE_HARNESS.md` updated to reflect Phase 4 macro loop (4a–4d orchestrator, CRAFT spawn, `auto_continue`, Desktop panel) as **shipped**. §6.7 adversarial auditor and 4e regression baseline remain accurately pending. P2 table updated accordingly.
  - **L2 §6.7 adversarial gap enumerator:** New `long_horizon/adversarial_audit.rs` — read-only agent-independent grounding signal (COMPOSABLE §0.1 class 2). Single `create_message` call (no tools); parses `[GAP]…[/GAP]` structured response for machine-testable gap candidates. Config: `[long_horizon.adversarial_audit]` `enabled=false` (opt-in), `mode="observe"|"enforce"`, `max_audit_rounds=1`, `max_tokens=1500`. Observe: emits `long_horizon.adversarial_audit` telemetry node. Enforce: gaps added as pending checklist items + reinject nudge. `NudgeAdversarialGaps` `LhtGateOutcome` variant; `LongHorizonContinueInput.llm_client`/`llm_model` fields. Design constraints preserved: no release/veto power, bounded per session. 7 unit tests.
  - **C1 CRAFT fix-loop Rust post-hook:** `executor.rs` now auto-spawns an Implementer sub-agent immediately after a CRAFT Review or Verifier completes with `BLOCKER`/`FAIL` verdict, without waiting for the parent model to parse the `<deepseek:craft.fix_loop>` sentinel. Closes the "靠模型自觉" reliability gap (Issue 8). New `craft::build_fix_loop_implementer_prompt()` builds structured remediation prompt with all blocker items. Guard: `MAX_CRAFT_FIX_LOOPS_PER_TASK` (3) cap preserved; `craft.fix_loop_auto_spawn` telemetry event emitted.
  - **C4 blackboard reviewer/verifier rounds:** `reviewer` and `verifier` blackboard partitions now accumulate `rounds[]` arrays mirroring `implementer.rounds[]`. Each round record captures `verdict`, `blockers_count`/`failures_count`, `summary`, and `evaluated_implementer_round`. Added `reviewer_round_count()` and `verifier_round_count()` public helpers. 1 new unit test (`test_reviewer_and_verifier_round_count`); backward-compatible (top-level `verdict`/`blockers` keys preserved).
  - **C8 pre-review executable spec gate:** `executor.rs` now runs `cargo fmt --check` → `cargo clippy -D warnings` → `cargo test --no-run` after every Implementer sub-agent completes successfully. If any step fails the gate spawns a new Implementer (capped at `MAX_CRAFT_FIX_LOOPS_PER_TASK`) with a structured prompt listing the failures instead of allowing broken code to reach the Reviewer. Emits `craft.pre_review_gate_fail` telemetry. New `craft::run_pre_review_gate()` (async, spawn_blocking) and `craft::build_gate_fail_implementer_prompt()`; gate skips gracefully if `cargo` is not in PATH.
  - **C3 Reviewer evidence gate:** `craft::enforce_reviewer_evidence_gate()` downgrades a Reviewer `BLOCKER` verdict to `MAJOR` when no `VerdictItem` contains a `[verify: cmd]` marker or recognisable shell-command token in its `suggestion` field. Applied in `executor.rs` before the blackboard write and C1 check; emits `craft.reviewer_evidence_downgrade` telemetry on downgrade. Prevents opinion-only BLOCKERs from triggering C1 auto-spawns. 6 unit tests (`evidence_gate_tests`).
  - **C9 CRAFT A/B metrics collector + runbook:** New `craft/ab_metrics.rs` module — `CraftAbRecord` appended to `.zagens/craft-ab-metrics.jsonl` (JSONL, schema v1) on every Verifier or terminal-Reviewer completion. Records `mode` (craft/single_agent), round counts, `terminal_verdict`, `evidence_downgrades`, `gate_fails`, and `duration_ms`. `CraftAbSummary::from_records()` provides aggregate stats. `executor.rs` triggers append automatically. `craft.rs` refactored into `craft/` directory module. `doc_Private/docs/harness/CRAFT_AB_RUNBOOK.md` — full A/B methodology, Python analysis snippet, and C1/C2 decision thresholds. 5 unit tests.
  - **C6(LSP) post-edit diagnostics:** New `craft/lsp_post_hook.rs` — after every successful Implementer sub-agent completion, spawns a background `cargo check --message-format=short` on the workspace and writes `lsp_diagnostics: {errors, warnings, lines[]}` to the CRAFT blackboard. Reviewer and Verifier `read_blackboard_section` injection now includes this section when errors/warnings are present. Skips gracefully when `cargo` is not in PATH. 4 unit tests.
  - **L3 `coverage-gate` CLI subcommand:** New `zagens coverage-gate [--workspace DIR] [--run-tests] [--require-checklist-complete] [--task-id ID] [--json] [--no-fail]` — cross-platform Layer-2 completion gate replacing PowerShell scripts. Runs `cargo fmt --check` → `cargo clippy -D warnings` → `cargo test --no-run` → (optionally) `cargo test` → checklist completeness check → CRAFT terminal verdict check. Exits 0/1 for pass/fail; `--no-fail` for report-only mode; `--json` for CI-parseable output. 6 unit tests.
  - **L7 headless regression + CI scheduling:** `scripts/ci/harness-regression.sh` — portable headless regression runner (mock-LLM safe; calls lib tests, CLI contract tests, e2e mock test, and `coverage-gate`). `.github/workflows/harness-regression.yml` — new scheduled workflow: daily regression job (~5 min, Ubuntu) + weekly stress job (~35 min, Ubuntu, uses `DEEPSEEK_API_KEY` secret, R-015 longrun baseline). `ci.yml` test job gains `coverage-gate --no-fail --json` report step (non-blocking) on Ubuntu.
  - **L8 Phase 4 completion (4e regression baseline + auto_continue fix):** `lht_presets.rs` `LongRefactor` preset: `auto_continue` corrected from `false` → `true` (required for macro-loop remediation segment to re-inject on stalled model). New `fixtures/harness/lht-eval-arms/lht_long_refactor.toml` eval arm for A/B baseline (LHT-only vs LHT+macro). `docs/harness/LONG_HORIZON_CODE_TASKS.md` Phase 4 table updated to "Shipped"; checklist items 4a–4e all marked complete; status header updated to 2026-06-14.

- **Runtime (kernel-v2 P2-A – P2-D + P2-Switch prep):** Context Compiler Phase 2 deliverables:
  - **P2-A** `ContextCompiler` skeleton in `zagens_core::engine::context_compiler` — `ContextSource`, `ContextLayer`, `BudgetPolicy`, `ContextProjection`, `CompiledContext`, `ContextCompilerMode` (`legacy`/`shadow`/`v2`); shadow mode wired in `model_request_fingerprint` with atomic diff counters and `GET /v1/runtime/kernel-shadow` endpoint.
  - **P2-B** Unified `TokenEstimator` in `zagens_core::engine::token_estimate` — single calibration authority for compiler budget accounting, capacity controller, and compaction trigger; three-path consistency test (≤1% max deviation).
  - **P2-C** Non-destructive compaction: `CompactionArtifact` + `ArtifactId` types in `zagens_core::compaction`; `compaction_artifacts` SQLite table (`CREATE TABLE IF NOT EXISTS`, additive migration); `compact_messages` returns `Option<CompactionArtifact>` with `replaced_messages_json` for full reversibility.
  - **P2-D** Overflow-recovery budget solver: `BudgetOverride`, `CompileError` in `context_compiler`; `compile_with_budget_override` two-phase eviction (Phase 1: Volatile sources lowest-priority-first; Phase 2: SemiStatic Elastic to `min`); `try_budget_recompile` stub on `Engine` as designated P2-Switch call site; step-latency gate < 10 ms.
  - **P2-Switch prep** Real source registration in `context_compiler_shadow`: `ContextCompilerStateSnapshot` captures `static_base_text` / `compaction_text` / `cycle_briefings_text` / `working_set_text` from live session; `build_compiler_from_snapshot` registers `system.static` (StaticPrefix/255), `memory.compaction` (SemiStatic/200), `memory.cycle` (SemiStatic/170), `working_set` (Volatile/160); `shadow_compare_with_snapshot` independently assembles system text from sources and computes `static_prefix_sha256` — diff tracking is now meaningful (not a re-fingerprint shortcut).

- **TUI (`zagens-tui`):** Composer `Ctrl+V` pastes from the system clipboard (also handles terminal `Paste` events); help and input hint updated.
- **TUI (`zagens-tui`):** Default **cool-blue** borderless layout — panel backgrounds distinguish left rail, transcript, composer, status, inspector, and LHT (no column divider lines). Extensible `TuiThemeId` presets (`cool-blue`, `gray-scale`, `charcoal`, `dracula-tint`, `high-contrast`, `classic`); persisted via `tui_theme` in `tui-layout.toml`. `AppState::cycle_tui_theme()` reserved for future UI/keybinding.
- **TUI (`zagens-tui`):** Composer `/lht` switches LHT composer mode (`auto` / `strict` / `off`, empty cycles) with picker UI; persists to `settings.toml` and shows current mode in the footer chip (applies on next turn).
- **TUI (`zagens-tui`):** Launch restores the last focused session in the current workspace (model, transcript, LHT state) via `tui-layout.toml` + thread store; use `--fresh` for a new session.
- **TUI (`zagens-tui`):** Right-rail inspector depth — Files file preview, Diff per-file patch, MCP tool expand, Agents cursor nav (`Enter`/`Esc`/`j`/`k`).
- **TUI (`zagens-tui`):** Composer `/model` (alias `/m`) switches the session text model with picker UI; persists via thread `model` field.
- **TUI (`zagens-tui`):** Activity marquee (1 row between Transcript and Composer) while the model is running — shows THK / tools / AI phase with animated rail.
- **TUI (`zagens-tui`):** Footer approval policy matches desktop Settings — four modes (`OnRequest`, `Untrusted`, `Never`, `Auto`); `Ctrl+A` cycles and persists to `config.toml`.
- **TUI (`zagens-tui`):** Optional `tui` feature and `zagens-tui` binary — full-screen three-column shell with in-process `RuntimeThreadManager` (`TuiSessionHost`), broadcast runtime event mapping (no `engine.rx_event` contention), Transcript (streaming, tools, thinking, harness lines, Markdown pipe-table grids), Composer (taller input, footer chips for model/mode/task/approve, blinking caret), approval modal, left-rail sessions, right-rail inspector tabs (files/diff/checklist/agents/MCP), harness checklist poll, high-contrast `theme.rs`, and 39+ `tui::` unit tests.
- **TUI (`zagens-tui`):** Right-rail inspector — `j`/`k` scroll (Files tab: move + expand target), `1`–`4` tab switch while right column focused, `Enter` toggles directory expand, `s` toggles staged vs worktree Diff; Files/Diff refresh on harness poll and turn end.
- **TUI (`zagens-tui`):** Right-rail **LHT lower pane** — splits inspector (Files/Diff/Agents/MCP) from collapsible LHT panel (objective · plan phases · checklist · blocked/nudge; no completion gate); auto-expands on `panel.checklist` / `harness.task_graph`; `l` toggle · `i` focus upper inspector.

### Fixed

- **Runtime (tool progress):** `exec_shell` / `task_shell_start` opening lines no longer panic when truncating commands that contain multibyte UTF-8 (e.g. Chinese in `[verify:]` checklist shells) — preview uses `floor_char_boundary` instead of a raw byte slice. File: `crates/core/src/engine/tool_progress.rs`.
- **TUI (`zagens-tui`):** Borderless sidebar seam artifacts (Windows) — 1-col black gutters, per-cell pane paint with full-width row backgrounds (selection/highlight extends entire row), column backfill + gutter re-stamp.
- **TUI (`zagens-tui`):** Right-rail (and all panes) — paint full pane background before text so stale `│` divider glyphs on the left edge are cleared on Windows (fixes Files panel 1-column ghost strip).
- **TUI (`zagens-tui`):** Sending a prompt no longer exits the whole app when `start_turn` fails — the error is shown in Transcript and the session stays open.
- **Desktop (Composer LHT off sync):** Cycling Composer to **LHT·关** now writes `lht_composer_mode = "off"` to `settings.toml` **and** syncs `config.toml` (`[long_horizon].enabled = false`, `macro_loop.enabled = false`). **LHT·严格** syncs `enabled=true` + `mode=strict`. **auto** leaves config unchanged. Files: `crates/config/src/{lht_config.rs,ui_settings.rs}`.
- **Runtime (scratchpad_set_area):** Non-empty `notes` on `scratchpad_set_area(done|deferred)` now auto-appends a `kind=meta` line when `notes.jsonl` has fewer than `require_min_notes` entries — fixes audit runs that batch `set_area(done)` with a summary in `notes` but skip `scratchpad_append`, which previously tripped loop-guard halt and ended the turn. File: `crates/runtime-server/src/scratchpad/mod.rs`.
- **Runtime (tool execution):** Fix sidecar panic in `tool_plans_exec` when a sub-batch runs one tool whose global index is not zero (`index out of bounds: len is 1 but the index is 1`). The worker crash aborted the turn mid-batch (e.g. `read_file` stuck in progress) and follow-up prompts failed with `Failed to start turn: channel closed`.
- **TUI (`zagens-tui`):** Center-column framing — Transcript/Composer borders and the faint dividers now all span the full column width (text stays inset by `CENTER_CONTENT_PAD`), so horizontal rules align and meet the side borders instead of appearing broken; the column reads as an enclosed pane (top border + full-width bottom rule + shared side borders).
- **TUI (`zagens-tui`):** Center-column dividers — removed the redundant double horizontal rule above the Composer (its titled top border is now the single separator); when a turn is live, one faint rule sits above the activity strip.
- **TUI (`zagens-tui`):** Refactored frame rendering into `tui/draw/` (`pane`, `chrome`, orchestration): one `paint_pane` path, explicit `BorderPlan` per region, and full-height column dividers painted on sidebar-owned columns (left rail right edge / right rail left edge) instead of inside the center stack.
- **TUI (`zagens-tui`):** Files tree — dropped the redundant leading cursor-prefix column (selection is shown via row highlight); file names now align with directory names right after the pane border instead of looking double-indented.
- **TUI (`zagens-tui`):** Right-rail inspector — file/diff/MCP/agent lines are truncated to the pane inner width and padded with sidebar background (no soft-wrap), eliminating right-edge ghost characters from long paths; sidebar panes render border + inner text separately.
- **TUI (`zagens-tui`):** Sidebar border repair no longer repaints over visible left/right rails (preserves top-left `┌` corners and fixes the intermittent left rule on first paint).
- **TUI (`zagens-tui`):** Transcript vertical rhythm — one blank row separates the user prompt from the agent's response block (THK / tools / AI); paragraph breaks (`\n\n`) inside assistant output are preserved as a single blank row (collapsing multiple blanks) instead of being stripped, while list items and section internals stay packed.
- **TUI (`zagens-tui`):** Input handling — dedicated blocking reader thread (fixes dropped keys during streaming); idle loop no longer polls at ~20–40 Hz.
- **TUI (`zagens-tui`):** Transcript scroll clamp (no blank screen when scrolled past top); per-turn tool collapse; THK block Enter to expand/collapse.
- **TUI (`zagens-tui`):** Session “Allow session” approval no longer escalates to full `trust_mode` / YOLO; API key from keyring injected into config (not `set_var`).
- **TUI (`zagens-tui`):** Mouse capture off by default (until scroll handlers exist); double Ctrl+C requires two presses within 1.5s.
- **TUI (`zagens-tui`):** Inspector file/diff detail cached on open; workspace thread list reuses canonical path.

### Added

- **TUI (`zagens-tui`):** Composer cursor editing (Left/Right/Home/End, Ctrl+W/U), prompt history (Up/Down), streaming prompt queue, approval `[v]` detail toggle.

- **TUI (`zagens-tui`):** Brighter idle/focus pane borders (`#555555` / `#777777`) and explicit black background on horizontal dividers for clearer layout on non-OLED terminals.
- **TUI (`zagens-tui`):** Composer top border follows chat focus like Transcript; help overlay no longer skips border repair; terminal resize clears the backing buffer to drop edge ghost cells.
- **Zagens (desktop sandbox settings):** Sandbox panel「完全访问」option wrote invalid `sandbox_mode = "full-access"`, crashing the sidecar on restart (`expected danger-full-access`); UI now uses `danger-full-access` and desktop normalizes legacy `full-access` on load/save.
- **TUI (`zagens-tui`):** Transcript layout stays compact — no extra blank rows between list lines, turn sections, or table data rows; markdown empty lines in prose are skipped.
- **TUI (`zagens-tui`):** Markdown ASCII tables render without `#1e1e1e` cell background — border rules use dim foreground, data rows match assistant text (fixes disconnected grid lines on black shell).
- **TUI (`zagens-tui`):** Fix intermittent broken vertical pane borders — clamp line padding to pane width (CJK-safe), LHT checklist uses display-width truncation, repaint border strips after each frame.
- **TUI (`zagens-tui`):** Composer, LHT, and inspector panes no longer show ghost characters from prior frames — each redraw clears the pane and pads lines to full width/height.
- **TUI (`zagens-tui`):** Fix center column side-border block painting over Transcript/Composer (input area invisible after divider layout change).
- **TUI (`zagens-tui`):** Tab into the right column now focuses the upper inspector pane (LHT auto-expand no longer steals subfocus); use `l` to expand LHT with lower-pane focus or `i` to return to inspector.
- **TUI (`zagens-tui`):** Checklist panel updates immediately on `panel.checklist` / `harness.task_graph` runtime events, auto-expands LHT lower pane, and polls during streaming on the event loop.

### Added

- **Runtime (Linux sandbox, kernel-v2 M0.4):** Optional Bubblewrap (bwrap) sandbox backend — opt-in via root config `prefer_bwrap = true` when `bubblewrap` is installed. exec_shell children get an enforced read-only root with write access limited to the policy's writable roots (`.zagens`/`.deepseek` metadata re-protected), `--unshare-all` with network shared back only when the policy allows it, and `enforced: true` reported on the exec path. Without bwrap (or on other platforms) behavior is unchanged (Landlock declare-only fallback). Adapted from upstream CodeWhale `sandbox/bwrap.rs` (MIT).
- **Runtime (providers, kernel-v2 M0.5):** `provider = "openai"` is now wired end-to-end in the runtime `ApiProvider` enum — previously it failed to parse and **silently fell back to DeepSeek** base URL and credentials. OpenAI gets its own `[providers.openai]` slot, `OPENAI_API_KEY` / `OPENAI_BASE_URL` / `OPENAI_MODEL` env overrides, canonical defaults (`gpt-4.1`, `https://api.openai.com/v1`), and free-form model passthrough (no DeepSeek model normalization). Facade `ProviderKind` ↔ runtime `ApiProvider` drift is now asserted by `config::providers::provider_drift_tests` (the runtime-only `deepseek-cn` regional alias is the one documented exception).
- **Harness (kernel-v2 corpus):** `kernel-v2-corpus-run.ps1` / `lht-harness-lib.ps1` fixes for Windows live replay — git CRLF stderr no longer aborts `git_init` seeding; REST JSON bodies send explicit UTF-8 bytes so Chinese prompts no longer fail sidecar parse with `invalid unicode code point`.
- **Runtime (kernel-v2 M2, partial):** `ToolManifest` / `Footprint` / `ResourceSet` / `SpawnClass` / `FootprintProvenance` in `zagens_tools`; `ToolSpec::manifest()` default derives conservative footprints from capabilities + M1 `tool_writes_state`; `McpToolAdapter` forces `provenance = McpSelfDeclared`. Registry acceptance test `agent_surface_tools_expose_conservative_builtin_manifest` covers the agent tool surface. **All production built-in tool families** now centralize model-visible `input_schema` in `tools/*_inputs.rs` + `tools/tool_schema.rs` with snapshot gate `fixtures/harness/kernel-v2-schema-snapshots/{file,search,shell,git,web,todo,scratchpad,subagent,task,github,office,misc,automation}-*.json` (84 snapshots, 13 gate tests) byte-identical after `schema_sanitize`. Complex payloads (`write_office`, `request_user_input`, `apply_patch`, `rlm`, …) keep hand-shaped JSON in the inputs module. **Remaining:** registry test-double schemas only.
- **Runtime (kernel-v2 M3, initial):** `PolicyEngine` in `zagens_tools` (`ApprovalNeed`, `SandboxClass`, `ParallelResourceKey`, `PolicyDecision`) with hard §8.1.2 rules for `FootprintProvenance::McpSelfDeclared` (self-declared read-only MCP tools still require approval and never enter parallel scheduling). Shadow bridge at `tool_plan_approval_meta` via `[tools] policy = "legacy" | "shadow" | "engine"` (default `legacy`); `shadow` records diffs to tracing + atomic counters without changing behavior. `diagnostics` exposes `policy_shadow` counters when shadow mode is active.
- **Runtime (kernel-v2 M5, initial):** `RequestFingerprint` (`static_prefix_sha256`, `full_prefix_sha256`) in `zagens_core::engine::request_fingerprint`; wire assembly in `runtime-server/src/request_fingerprint.rs` hashes static system layer (through compaction template) + tool catalog separately from full system + tools + wire messages. Each model step emits `turn.prefix_fingerprint` runtime events (via `Event::ModelRequestPrepared` + monitor); `tracing` target `kv_cache` logs the same hashes. CI: `scripts/kernel_v2_prefix_ci.py` → `cargo test -p zagens-cli request_fingerprint` (includes kernel-v2 `workspace-seed` fixture). Corpus report adds `prefix_fingerprint_summary`; live replay gate `--assert-prefix-stability --require-fingerprints` on `kernel-v2-corpus-run.ps1`.
- **Runtime (kernel-v2 M4, initial):** Resource-dependency DAG scheduler in `zagens_tools::dag_scheduler` (`build_execution_waves`, path/workspace scan resources); `schedule_bridge` maps `ToolExecutionPlan` → `DagPlanView` with §8.1.1 shell dynamic footprint (`analyze_command` + `ShellManager::probe_sandbox_enforced`). `[tools] scheduler = legacy | shadow | dag` (default `legacy`); shadow records legacy vs DAG group diffs; DAG mode splits each wave into parallel auto-eligible batch + serial approval/write sub-groups. Fine-grained per-resource async locks (`resource_locks` + shared `ResourceLockRegistry` on `EngineRuntimeExt`) replace the global batch lock when `scheduler = dag`, allowing concurrent reads on distinct paths within a wave. Corpus: `kernel-v2-corpus-run.ps1 -ToolsScheduler dag` and `scripts/kernel_v2_corpus_compare.py` for M4 −20% pure_read gate.
- **Runtime (kernel-v2 M3 bake ops):** `GET /v1/runtime/kernel-shadow` exposes process-local `policy_shadow` / `scheduler_shadow` counters when `[tools] policy` or `scheduler` is `shadow`. Corpus runner probes this before each sidecar stops and records counters in `runs.jsonl`; `scripts/kernel_v2_shadow_bake_report.py` aggregates bake diff rates (M3 gate: &lt;0.1%).
- **Harness (kernel-v2 mode smoke):** `scripts/kernel-v2-mode-smoke.ps1` replays one corpus scenario per policy/scheduler combination with isolated merged configs and prints a pass/fail summary (shadow diff counts when applicable).

### Changed

- **Runtime (kernel-v2 M1):** Unified mutating-tool predicate in `zagens_core::engine::tool_effects::tool_writes_state` — conservative union of the former `LoopGuard::is_state_mutating_tool` whitelist and `tool_bridge::tool_name_is_mutating` heuristic (includes `exec_shell` / `exec_shell_wait` / `exec_shell_interact`). **Behavior change:** successful shell execution now clears identical-call loop-guard counters (same as file edits), so `edit → re-run same test command` no longer trips the 3× identical-call block after the shell succeeds; hammering the same shell with no intervening state change still blocks. Golden test `golden_builtin_tool_writes_state` covers 95 built-in tool names.
- **Runtime (token estimation, kernel-v2 M0.3):** Unified the two divergent text→token calibrations (core `ceil(chars/3)` vs compaction DeepSeek CJK ratios) into a single entry `zagens_core::engine::token_estimate::estimate_text_tokens` returning the conservative union (never lower than either). UI usage, capacity ratios, and compaction thresholds now read the same calibration; effect: compaction may trigger slightly earlier on ASCII-heavy histories (~+11% estimate), context trim is more conservative for CJK-heavy content. Cross-crate consistency is asserted by `compaction::unified_calibration_core_and_compaction_agree`. Pure-ASCII text takes a vectorized fast path (`is_ascii` + `len/3`), keeping repeated whole-session estimates (context trim, capacity checkpoints) at pre-unification speed on large ASCII-heavy histories.
- **TUI (`zagens-tui`):** Pure-black shell (`#000000`) with faint pane borders; transcript section spacers removed; center column uses horizontal rules between Transcript, activity marquee, Composer input, and footer chips; readable text inset `CENTER_CONTENT_PAD` from side borders.
- **TUI (`zagens-tui`):** Agent reply body uses `#50fa7b`; semantic Dracula tokens for roles/tools/THK; sidebar shares pure-black shell with faint borders (`TUI方案.md` §6.10 maintainer copy).
- **TUI (`zagens-tui`):** Fix streaming transcript overlap — CJK assistant lines use hanging indent + single-span render + Dracula background fill so terminal soft-wrap does not cross rows under tool `+` column.
- **Runtime (Windows exec_shell):** Sandboxed shell children inherit the parent process environment by default (Codex-aligned `inherit: all`, with secret-name filtering), so MSVC/SDK vars (`LIB`, `INCLUDE`, …) reach `cargo build` when the Zagens parent has them. `workspace-write` now treats `%TEMP%` / `%TMP%` as writable roots on Windows. `diagnostics` reports `exec_shell_env_inherit` and whether the parent exposes toolchain env.
- **Web tools (research quality):** `web_search` default/max results 8/15; Tavily `search_depth: advanced`; `fetch_url` uses block-aware HTML extraction; `web.run` shows more lines per `open`, honors `[search] provider` for `search_query` (metaso/tavily/etc.), and tool/prompt text enforces search-then-`fetch_url` two-step. Bundled `multi-search-engine` skill aligned to `fetch_url`.
- **CI PR-first:** Remote CI runs on pull requests and release tags only — not on merges to `master` / `main`. Local pre-push still uses `scripts/ci/ci-push-gate.sh` to skip lint for docs-only or `[skip ci]` landings.
- **Desktop (chat):** Chat bubbles no longer render Mermaid inline — ` ```mermaid ` ` blocks display as fenced code; use the right-panel Mermaid tab for diagram preview.
- **Desktop (Mermaid preview):** Workspace Markdown preview and Mermaid panel use trusted rendering (Cursor-like direct SVG mount, no DOMPurify on diagram output); lightweight SVG threat scan blocks preview until the user opts in. Chat Markdown and diff output still use DOMPurify.

### Added

- **Desktop (Composer):** Pasting a lone http(s) URL (including GitHub rich-link copies) adds a Cursor-style link chip above the input instead of dumping page title text; referenced URLs are sent to the model with a `fetch_url` hint.
- **Desktop (preview):** Markdown file preview renders embedded ` ```mermaid ` ` fences inline (same engine as the Mermaid panel).
- **Desktop (Mermaid):** `npm run test:mermaid` self-check covers digest, `<br/>` normalization, fence plugin, and SVG sanitization.

### Fixed

- **Harness (kernel-v2 corpus):** `kernel-v2-corpus-run.ps1` no longer injects a duplicate `[tools]` table when the merged base config already has one; allows runs when API key is only in the OS keyring (warns instead of requiring `DEEPSEEK_API_KEY` in env when `~/.zagens/config.toml` exists).
- **Harness (kernel-v2 corpus):** `kernel_v2_corpus_report.py` SSE parser splits on `\n` only instead of `str.splitlines()` so UTF-8 CJK prompts (e.g. 阅 `E9 98 85`) are not broken at the U+0085 byte — previously dropped `turn.started` / `turn.completed` and reported zero step latencies on Chinese scenarios.
- **Harness (kernel-v2 M5):** Live corpus `--assert-prefix-stability` now checks static-prefix stability on **`pure_read` only** (default `--prefix-stability-shapes pure_read`); write/shell turns may activate deferred tools mid-turn and legitimately change the tool-catalog slice in `static_prefix_sha256`.
- **Desktop (build):** `bundle:prepare` installs web-ui devDependencies when `NODE_ENV=production` omitted `tsc`/`vite` (fixes `'tsc' 不是内部或外部命令` on Windows).
- **Desktop (Mermaid):** Preserve `foreignObject` label HTML when sanitizing rendered SVG so mindmap (and similar) node text is visible again after DOMPurify 3.1.7+.
- **Desktop (Mermaid preview):** Render flowcharts with SVG-native labels (`htmlLabels: false`) so Tauri WebView2 no longer shows black rectangles over complex diagrams.
- **Desktop (Mermaid preview):** Match Cursor/GitHub with `htmlLabels: true`; promote `fill`/`stroke` to SVG attributes, inline edge `stroke`, and transparent `foreignObject` label backgrounds for WebView2; Markdown preview mounts diagrams in an isolated iframe at native viewBox size (fixes `foreignObject` label overlap and missing connectors in `RUNTIME_ARCHITECTURE.md`).
- **Desktop (Mermaid preview):** Scale Markdown preview diagrams to fit pane width (uniform iframe transform) and inline `foreignObject` label text colors so light nodes show dark text and classDef nodes show white text in WebView2.
- **Desktop (Mermaid preview):** Fit Markdown preview iframe height via SVG `width`/`height` + `viewBox` (replaces `transform: scale`, which clipped diagrams in WebView2).
- **Desktop (Mermaid preview):** Sync `foreignObject` HTML font size to diagram fit scale and restore edge-label `labelBkg` backgrounds so light node text and connector labels (e.g. `spawn + DS_PICK_READY`) render in WebView2.
- **Desktop (Mermaid preview):** Fit Markdown diagrams with uniform CSS `zoom` so node boxes, subgraph titles, and `foreignObject` labels scale together (fixes bottom-clipped light-node text in WebView2).
- **Desktop (Mermaid preview):** Force `-webkit-text-fill-color` on `foreignObject` labels and transparent foreignObject backgrounds so light-node, subgraph, and edge text are visible in WebView2 (theme `fill:` no longer paints white invisible glyphs).
- **Desktop (Mermaid preview):** Derive label text color from node shape `fill` luminance for unclassed `node default` syntax (e.g. §1.1 `U1`/`U2`) instead of requiring doc-side `classDef`.
- **Desktop (Mermaid):** Full i18n for panel + inline render paths (en / zh-Hans / ja / pt-BR); inline error retry button; theme-change re-render; empty fence placeholder (no flash); Mermaid panel resize debounce retained.
- **Release (crates.io):** Include `zagens-windows-sandbox` in `publish-crates.sh` before `zagens-cli`; add workspace `repository`/`homepage`/`readme` to its manifest; pre-publish leaf dry-run covers it; post-publish verify uses workspace version.

## [0.7.5] - 2026-06-11

### Fixed

- **CLI (Linux CI):** Gate `zagens sandbox` handlers and Windows-only config re-exports with `#[cfg(windows)]` so `zagens-cli` builds on Ubuntu with `-D warnings`.

## [0.7.4] - 2026-06-11

### Added

- **Session restore UX:** Chat shows restore progress, data source (cache / thread replay / session archive), degraded-session warning, and a **Reload** action to retry thread event replay.
- **CRAFT sub-agent models:** `[subagents]` accepts `implementer_model`, `verifier_model`, and `auditor_model` (wired like `review_model`); Zagens **Settings → Security** exposes optional per-role model dropdowns.
- **Windows sandbox Phase 2 (elevated):** `zagens-sandbox-setup.exe` + `zagens-command-runner.exe` helpers; offline/online sandbox users; WFP outbound blocks; grant-read / deny-read profile ACLs; DPAPI secrets; full teardown; elevated sync/background `exec_shell` via IPC; structured `sandbox_denial_code` metadata; helper bundling via `bundle:prepare`.
- **Windows sandbox Phase 3:** ConPTY interactive `exec_shell` (`tty: true`) through elevated command-runner; optional `[windows] sandbox_private_desktop`; CLI `zagens sandbox add-read-dir`; enterprise `requirements.toml` knobs; Gate **G2** acceptance probes (`conpty_echo`, `add_read_dir`, background IPC, teardown verify — 14/14 when setup complete).
- **Desktop sandbox UX:** sidebar **Sandbox** settings panel; Windows first-run onboarding wizard (`sandbox_initialized`); `diagnostics` / `exec_shell` report configured vs effective Windows sandbox mode.

### Changed

- **Desktop chat UX:** Transcript uses lightweight meta bars for reasoning/tools (collapsed by default), tool-call aggregation labels, improved assistant paragraph breaks and typography, minimal new-session empty state, and composer/sidebar spacing alignment.

### Fixed

- **Session tool persistence:** `reconstruct_messages_for_store` now embeds `tool_use` / `tool_result` blocks in session JSON; `seed_thread_from_messages` recreates tool turn items and emits replay events so resumed threads restore tool cards without localStorage.
- **Desktop session restore:** Prefer thread/cache snapshots over fragmented session JSON when picking chat history after restart (richness scoring).
- **Audit scratchpad P2:** Block batched `scratchpad_set_area(deferred)` in one model step; enrich defer prerequisite errors and P2 continue nudges with pending `area_id` list and one-area-per-step workflow (fixes CRAFT audit sessions stalling on mass defer failures).
- **Engine op loop:** Restore disjoint-field platform dispatch so `Engine::ext` stays populated during `dispatch_op` — fixes `Failed to start turn: channel closed` after a security refactor left `runtime_ext_mut()` with an empty `ext` slot.
- **CRAFT blackboard:** Implementer partition writes `{ "rounds": [...] }` (not a bare array); `changes` use `{ file, intent }` with `git diff --name-only HEAD`; legacy bare-array boards still read correctly.
- **CRAFT fix-loop:** Circuit breaker after 3 Implementer rounds per `task_id` emits `<deepseek:craft.fix_loop_exhausted>` (`escalate_user`).
- **CRAFT:** mtime-keyed blackboard read cache; `craft-verdict` truncated JSON salvage; `craft_notice` when CRAFT spawns omit `task_id`.
- **Windows unelevated (G1):** native PowerShell/cmd execution (no broken `cmd /C` double-quoting); correct spawn CWD (`\\?\` strip, relative cwd join); background stdin + `taskkill /T` tree kill; per-spawn workspace ACL grants.
- **Windows elevated (G2):** ConPTY EOF deadlock; stale command-runner materialization; runner bootstrap CWD/path (Win32 267/5); offline WFP catch-all + loopback permit; HANDLE_LIST / stdin fallback for restricted-token spawns.

### Changed

- **Windows sandbox default:** after `zagens sandbox setup`, unset `[windows] sandbox` defaults to **elevated**; incomplete setup falls back to unelevated with a warning. Settings copy distinguishes enforced vs setup-required.
- **Docs:** `docs/tech/SANDBOX_CAPABILITY_MATRIX.md` updated for elevated/unelevated G2 semantics.

### Security

- **Audit follow-up:** stdio MCP `server/register` rejects `command`/`args`/`env`; lifecycle snapshots no longer echo executable fields; scratchpad `reviewed_ratio` hard gate (default 40%) for audit deliverables; `audit-repo` skill v2.2 report template.
- **Windows unelevated honesty:** cap-SID deny-read does **not** block reads under `WRITE_RESTRICTED` tokens — unelevated mode delivers write isolation + best-effort network restriction only; profile read isolation requires the elevated sandbox-user path (Gate G0 documented as fail).

## [0.7.3] - 2026-06-09

### Added

- **Tasks panel:** expandable task cards load `GET /v1/tasks/:id` to show prompt, Agent reply summary, execution timeline, tool calls, and errors; **Open full conversation in chat** loads the task's runtime thread into the main composer. Running tasks auto-refresh while expanded.
- **Desktop Composer model picker:** reads model IDs from `config.toml` (`available_models` / `default_text_model`), supports custom provider model strings; Settings default model is a free-text field synced on save.

### Changed

- **Desktop Composer presets:** removed legacy `deepseek-chat` / `deepseek-reasoner` from the model picker (DeepSeek V4 Pro / Flash only); runtime still normalizes those IDs when present in config or API payloads.

### Security

- **P0-1 Deadlock fix** (`runtime-orchestrator`): `list_pending_session_inputs` re-locked `std::sync::Mutex` while guard alive causing guaranteed deadlock on SQLite backends.
- **P0-2 Topic memory injection fix**: User-derived text in `blocked_points.context` now markdown-escaped before system prompt injection.
- **P0-3 Compaction injection fix**: LLM-generated summary/workflow text now XML-escaped and wrapped in containment tags before system prompt injection.
- **P0-4 SSRF fix**: IPv4-compatible IPv6 addresses (`::127.0.0.1`) now blocked by `is_restricted_ip` alongside IPv4-mapped.
- **P1-5 Sandbox degraded notice**: `mark_sandbox_policy_unenforced` now emits `tracing::warn!` with the degraded-mode notice so operators see it in logs.
- **P1-7 Config API key redaction**: `ConfigToml::get_value()` and `list_values()` redact all API key fields (root, per-provider, vision) via `redact_secret()`; manual `Debug` impl redacts secrets in logs.
- **P1-8 Thread fork I/O**: `fork_thread` / `fork_at_user_message` SQLite work runs on `spawn_blocking` so async workers are not blocked.
- **P2-10 ExecPolicy path flags**: prefix-allowed commands with `--manifest-path` / `--config` outside the workspace are denied after allow matching.
- **P2-10 Hooks error observability**: `HookDispatcher::emit` logs sink failures at `tracing::warn!` instead of silently discarding errors.
- **P2-11 Approval cache fields**: `ApprovalResult::Approved` carries `cache_key` / `remember_for_session` through `zagens-core::await_tool_approval`.
- **P1-6 sandbox_mode wiring**: User-configured `sandbox_mode` (`read-only`, `workspace-write`, etc.) now feeds into `SandboxPolicy` via the tool context; the stricter of the user setting and AppMode default is used (YOLO still grants DangerFullAccess).
- **P2-9 SandboxBackend cwd**: `SandboxBackend::exec` trait now accepts `cwd: Option<&Path>`; external sandbox backend passes the user-requested working directory.
- **P2-13 Desktop hardening**: `openExternalUrl` blocks non-`http/https/mailto` schemes; `Composer` dropped `dangerouslySetInnerHTML`; IPC `read_workspace_binary_at_root` / pick-rules paths must stay under the user home or documents directory.
- **P2-12 Deprecation**: `crates/agent` now has a README marking it deprecated/legacy.

### Fixed

- **LHT panel (task timer):** the 长程任务 stopwatch again ticks for the full incomplete task (including `exec_shell` / `cargo clippy` gaps between composer streaming chunks), not only while `pollFast`/「生成中」; still freezes at 100% and accumulates across reinject rounds on the same thread.

## [0.7.2] - 2026-06-09

### Added

- **Desktop settings — Hooks & scheduled tasks:** Lifecycle hooks panel (`config.toml` `[hooks]`) and RRULE-based scheduled automations UI under Settings; sidecar scheduler and hook execution wired for session/tool/subagent events.

### Fixed

- **Hooks execution (P0):** Per-hook timeout now takes priority over `default_timeout_secs`; background hooks receive JSON stdin and enforce timeout; blocking events ignore `background=true` so deny/modify can work.
- **Hooks & automations (P1/P2):** `tool_call_after` exit-code conditions; scheduler batch catch-up after downtime; orphan automation runs marked failed on sidecar restart; `before_shell` alias implicit conditions preserved via IPC; HooksPanel load/save feedback.
- **Hooks polish:** `pre_compact` sets `compaction_manual`; subagent start/end hooks receive model id; empty hook command warning in UI; remove dead code in `run_now`.
- **Desktop onboarding:** Persist first-run completion across restarts; migrate legacy `deepseek-desktop-*` localStorage keys to `zagens-desktop-*`.

## [0.7.1] - 2026-06-08

### Fixed

- **Desktop onboarding:** Persist first-run completion and default task type to `~/.zagens/settings.toml` (not only WebView `localStorage`); returning users see the startup splash only, not the mode wizard every launch.
- **Desktop stop button:** First click on「停止」now reliably ends the turn (no second click required).
- **Sub-agent panel:** Scope sub-agent history by parent runtime thread (`parent_thread_id` persisted + UI hydration on thread switch).

## [0.7.0] - 2026-06-08

First public open-source release on [GitHub](https://github.com/didclawapp-ai/zagens) (MIT).

### Highlights

- **Zagens desktop (Windows)** — Tauri 2 agent harness for code and office workspaces (sidecar, LHT/CRAFT, Office tools, embedded terminal).
- **Headless CLI (`zagens`)** — scriptable `exec`, `review`, `apply`, `doctor`, `setup`, `serve --http`, MCP helpers; crate **`zagens-cli`**, sidecar binary **`zagens-runtime`** unchanged for desktop embed.
- **GitHub Release `zagens-v0.7.0`** — Windows installer (zip + SHA-256) and cross-platform CLI binaries when CD succeeds. Install from source until assets appear: [LOCAL_DEV_VERIFY.md](LOCAL_DEV_VERIFY.md).

### Added

- **Headless CLI (`zagens`):** New scriptable binary in `crates/runtime-server` (`src/bin/zagens.rs`) with MVP commands — `exec`, `review`, `apply`, `doctor`, `setup`, `login`/`logout`, `models`, `mcp list`/`tools`, `completions`, `serve --http`. Desktop sidecar binary **`zagens-runtime`** and `main.rs` unchanged. Install: `cargo install zagens-cli --bin zagens` or `cargo build -p zagens-cli --bin zagens`. Docs: README § Headless CLI; Scoop template `packaging/scoop/zagens.json`.
- **Headless CLI tests:** Binary contract tests (`tests/zagens_cli_contract.rs`), desktop sidecar argv smoke (`desktop_sidecar_spawn_argv_contract`), `apply` git fixture in `cli/tests.rs`, CI jobs on Ubuntu.

### Changed

- **Brand / crates.io:** Rename runtime package `zagens-cli` → **`zagens-cli`** (`cargo install zagens-cli --bin zagens`). `/health` service id → `zagens-runtime-api`; desktop i18n and headless prompts use `zagens-runtime` / **Zagens**. Future full-screen TUI binary: **`zagens-tui`**.

- **Open source:** Public GitHub repository (`didclawapp-ai/zagens`, MIT); user docs on [zagens.com/docs](https://zagens.com/docs); internal maintainer docs stay local-only (`doc_Private/`).

- **CI/Release hardening (`.github/workflows/`):** Pin the toolchain action to `dtolnay/rust-toolchain@1.96.0` (was `@stable`) across all jobs so clippy/rustfmt components match `rust-toolchain.toml` instead of relying on rustup auto-switch; add least-privilege `permissions: contents: read` (publish job keeps its `contents: write` override) and `concurrency` groups (CI cancels superseded ref runs except scheduled; Release never cancels in-flight); drop the unused `actions/setup-node` step from the CI `versions` job. **Release now gates on a `verify` job** (version drift + fmt + strict clippy + workspace tests) before building/publishing the Windows installer, so a tag on an unverified commit can't ship a broken release.
- **CD (`.github/workflows/cd.yml`):** Tag `zagens-v*` → CI full matrix → build Windows installer + CLI binaries → **GitHub Releases only** (removed zagens.com sync). Manual `workflow_dispatch` builds artifacts without publishing.

### Fixed

- **CI (Ubuntu release build):** Reclaim GHA disk (free preinstalled packages, drop `target/debug` before release smoke) — fixes `No space left on device` during `cargo build --release`.
- **CI (Windows):** Run `zagens-cli` lib tests only in the matrix job; keep spawn/binary contract tests on `ubuntu-latest` — avoids 40+ minute flakes under full workspace load.
- **CI (Linux link):** Add 4G swap on Ubuntu runners for `zagens-runtime` lld link OOM (`Bus error`).

- **CI (crates.io flakes):** Add `scripts/ci/cargo-retry.sh` and use it for `cargo fetch`/build/clippy/test in CI and Release verify — mitigates transient `curl 56` / connection-reset failures when downloading crates on GH Actions (especially `windows-latest`).
- **CI lint (desktop manifest):** Drop redundant `license-file` from `crates/desktop/Cargo.toml`; use `license.workspace = true` (MIT SPDX) so Cargo stops warning on every job.
- **CI test (runtime-server, macOS):** Poll for `turn.completed` / steer events in thread contract tests instead of reading events immediately after `wait_turn_terminal` — the event is appended after the terminal save, which caused `turn_completed_event_includes_turn_summary` to flake on CI.
- **CI lint (desktop, 1.96 strict clippy):** Fix `-D warnings` failures in `crates/desktop`: scope the `OnceLock` import into the `#[cfg(windows)]` `windows_shell_exe()` (unused on Linux) in `terminal.rs`; drop a needless `return` and widen `statvfs` fields via `u128` multiply in `disk_guard.rs` (avoids `as u64` on Linux and `u64::from` useless-conversion on the same); collapse a nested `if` into a `let`-chain in `sidecar.rs`.
- **CI test (runtime-adapters, unix):** Import `StdioTransport`, `STDIO_SHUTDOWN_GRACE`, and `Duration` in `stdio_transport_shutdown_terminates_child` (`mcp/tests.inc.rs`); expose `STDIO_SHUTDOWN_GRACE` as `pub(crate)` — the test is `#[cfg(unix)]` only so Windows local lint never compiled it.
- **CI test (macOS/Windows):** Run `ensure-web-ui-dist.sh` before `cargo test --workspace` on all Test matrix runners; `generate_context!()` requires `crates/desktop/web-ui/dist` (Lint job already built it on Linux only).
- **CI test (runtime-adapters):** Align `test_mcp_config_defaults` with `connect_timeout` default **30** s (was still asserting 10 after the MCP timeout bump).
- **CI lint:** Remove needless `return` statements in `policy_degraded_mode_notice()` (`crates/runtime-server/src/sandbox/mod.rs`) — resolves `clippy::needless_return` errors that broke CI on push.
- **CI lint:** Pre-build `zagens-cli` before `cargo clippy --workspace` in `.github/workflows/ci.yml` and `scripts/ci/verify-lint.sh`; `crates/desktop/build.rs` requires the sidecar binary at compile time (same as the Test job fix below).
- **CI:** Test job now `needs: lint` so fmt/clippy failures skip the three-platform test matrix and save CI minutes.
- **CI test (macOS/Windows):** Pre-build `zagens-cli` before `cargo test --workspace` in `.github/workflows/ci.yml`; `crates/desktop/build.rs` requires the sidecar binary in `target/debug/` at compile time, causing build failure on all non-Linux runners when the binary wasn't yet present.
- **Desktop build.rs:** Add `ensure_resource_stubs()` — creates empty stub directories for gitignored Tauri resources (`binaries/python-standalone/python-install`, `bundle-legal/`) so `tauri-build` resource-path validation passes during `cargo test` / `cargo clippy` without the release artifacts on disk.
- **CI lint (macOS):** Remove redundant top-level `#[cfg(unix)] use std::os::unix::process::CommandExt` from `crates/runtime-server/src/tools/shell/process.rs`; the trait is already imported locally inside `install_parent_death_signal` (Linux-only), making the file-level import unused and triggering `-D unused-imports`.
- **Fix (CI):** Run version/OpenAPI scripts via `bash` (Windows checkout lacks `+x`); install Tauri Linux deps (`libwebkit2gtk-4.1-dev`, …) via `scripts/ci/install-linux-deps.sh`; re-sync OpenAPI + `runtime-api.ts` usage cache telemetry fields.
- **Fix (CI):** `cargo fmt` for `crates/topic-memory` (stopwords list); move Windows-only path strip under `#[cfg(windows)]` (fixes macOS `-D warnings` unused `s`).
- **Fix (CI):** `cargo fmt --all` — `sidecar_binary_contract.rs` assert line-break, `skill_install.rs` use import, and workspace-wide rustfmt drift after main-repo push.
- **Fix (CI):** add Unix-only `libc` dep to `zagens-runtime-adapters` for `StdioTransport::shutdown` SIGTERM (`mcp/transport.rs`; fixes macOS Test job E0433).
- **Fix (CI):** collapse nested `if` in `crates/config/src/ui_settings.rs` (`clippy::collapsible_if`).
- **Fix (CI):** `zagens-core` clippy — `too_many_arguments` on turn-loop host/phase fns, `collapsible_if` in `project_context`, `needless_borrow` / `should_implement_trait` allows.
- **Fix (CI):** `zagens-runtime-adapters` clippy — `double_must_use`, `collapsible_if`, `needless_question_mark`, `io_other_error`, `needless_borrow(s)`.
- **Tooling:** Pin dev/CI Rust **1.96** via [`rust-toolchain.toml`](rust-toolchain.toml); add [`scripts/ci/verify-lint.sh`](scripts/ci/verify-lint.sh) / [`verify-workspace.sh`](scripts/ci/verify-workspace.sh) and optional git hooks ([`scripts/ci/install-git-hooks.sh`](scripts/ci/install-git-hooks.sh)) so fmt/clippy fail locally before push.
- **Fix (CI):** `topic-memory` + `runtime-orchestrator` clippy under Rust 1.96; lint job builds `web-ui/dist` when missing ([`scripts/ci/ensure-web-ui-dist.sh`](scripts/ci/ensure-web-ui-dist.sh)).

### Docs — CMS 存量审计测试案例（CMS-AUDIT / CMS02）

- **Docs:** 新增 [`docs/harness/test-cases/cms-full-code-audit.md`](docs/harness/test-cases/cms-full-code-audit.md) — `F:\CMS框架` 首跑实证（221 源文件、19 区域、12 Explore、5 项 verify、HIGH×13、审修闭环）；含可复制 prompt、判定矩阵、OpenCode 对标字段与 CMS-L/XL 规模梯度预留。
- **Docs:** 新增 [`docs/harness/fixtures/cms-audit-completion-gate.toml`](docs/harness/fixtures/cms-audit-completion-gate.toml)（`tsc`/`vue-tsc`/报告存在门）；[`LHT_TEST_SUITE.md`](docs/harness/LHT_TEST_SUITE.md) §2.2 + §3、[`OPENCODE_AGENT_CORE_BENCHMARK.md`](docs/tech/OPENCODE_AGENT_CORE_BENCHMARK.md) §4.1 交叉引用。
- **Docs:** CMS02 **全链路终态归档** — §9 实证明细（verify 多轮真绿、Prisma generated 判别、19 处类型清理、P0×5+P1×3、9/9 checklist·100%、§9.5 诚实边界）；测试集与 OpenCode §4.1 摘要同步更新。

### Runtime — sub-agent display names

- **Change (sub-agent nicknames):** Replaced whale-species rotation with task-oriented labels: explicit `nickname` / `display_name` on `agent_spawn`, else derive from `area_id`, `## Audit Task:` title, `task_id`, spawn `cwd` basename, then `{type} #N`. Audit parallel spawns now show scopes like `BE-Services` in the agent panel title.

### Zagens desktop — composer Stop after start failure

- **Fix (Stop stuck after `channel closed`):** When `startThreadTurn` failed (e.g. `Failed to start turn: channel closed`), the UI could stay in「生成中」and **Stop** did nothing — `finishOnce` re-locked against a zombie backend `active_turn`, and Stop called the same path. Local finish now supports `force` (user Stop + HTTP start errors skip re-lock); Stop resolves `latest_turn_id` when `turnId` was never assigned. Runtime rolls back in-memory `active_turn` and persists `Failed` when `engine.start_turn` never starts.

### Docs — OpenCode agent 核心对标

- **Docs:** 新增 [`docs/tech/OPENCODE_AGENT_CORE_BENCHMARK.md`](docs/tech/OPENCODE_AGENT_CORE_BENCHMARK.md) — OpenCode（`dev`）与 Zagens agent 核心对照、已借鉴项、ROI 排序建议与 P0–P4 路线图。
- **Docs:** 审核修订对标文档 — 修正 V2 eager 工具表述、区分 V1/V2 doom loop、补充 Desktop Beta 竞争注记与 harness 借鉴边界。
- **Runtime (P0+P1):** `TurnCoordinator` per-thread drain 串行化；SQLite `session_input` durable inbox + `StartTurnRequest.delivery`（`queue` 时 202 + `queued`）；turn 结束后自动 drain queue。P2–P4 冻结。

### Zagens desktop — 会话历史恢复

- **Fix (会话历史):** 切换会话时 best-effort `persist-session` 落盘 outgoing thread，避免未持久化回合在重启后丢失。
- **Fix (会话历史):** 恢复时从 cache / session JSON / thread 事件三路取最完整快照，避免 thread 回放较短时覆盖更完整的本地缓存。
- **Fix (会话历史):** `sessionUiCache` 改为 LRU 淘汰（不再按 Object.keys 顺序误删活跃会话）；窗口隐藏时对任意已恢复 thread 触发 persist（不再仅限 streaming）。
- **Fix (会话历史):** thread 事件回放时在 `agent_message` 完成与 `turn.completed` 立即 flush assistant，避免跨 turn 合并导致消息截断。

### Zagens desktop — 长程任务计时

- **Fix (LongHorizonPanel):** 任务图计时改为绑定 composer **会话进行中**（`streaming` /「生成中」），不再在 checklist 100% 时冻结；主 turn 结束后冻结，同一 thread 内 LHT 多轮复跑**累计**不重置。

### Runtime / Zagens — LHT+CRAFT 宏观循环触发扩展

- **Feature (LHT Phase 4):** CRAFT 宏观循环新增进入时机 `on_graph_complete`（checklist 已勾完、微观门可未绿）与 `on_manifest_exhausted`（manifest 轮次诚实耗尽）；`long-refactor` 预置默认改为 `on_graph_complete`。微观门未绿时也可 spawn CRAFT；补全段注入 `MacroRemediation` 继续修缺口，并提示须重新通过微观验收门。
- **Fix (LHT+CRAFT 闭环):** CRAFT spawn 后 turn 在收尾前等待 review 子代理完成；若 turn 已结束但 blackboard 有 blockers，下轮 `maybe_inject` 自动 `try_resume_pending_macro_remediation` 注入补全段。补全 nudge 合并 `macro_pending_manifest_hints`（微观门失败摘要）。
- **Fix (LHT Go 假绿):** plan-bootstrap nudge 附加 Go harness 规划提示（per-package 覆盖率、`cmd/*` 可测布局）；manifest 失败 nudge 与 `go_toolchain_audit` 对 `cmd/`/`examples/` 低覆盖给出可操作建议。
- **UX (LHT 设置):** 启用宏观审查循环时展示 token/API 费用增高提示（i18n ×4）。

### Runtime — LHT 假绿加固（MicroStack 日志复盘）

- **Fix (LHT P0-3):** `go test` toolchain 门不再对全 `[no test files]` 假绿；`go test -cover` 解析覆盖率，默认 **≥60%** 才过（`go_toolchain_audit.rs`）。工具链探测改为 `go test -cover ./...`。
- **Fix (LHT P0-3):** checklist ≥5 项且 **零** `[verify:]` 标签时，在 `graph_complete` 前注入 `insufficient_verify_nudge`（阻断「只 build/vet 就收尾」）。
- **Fix (LHT):** `stub_gate` 跳过 `crates/runtime-server/src/long_horizon/` 等 harness 基础设施路径，避免在 Zagens  monorepo 内自举分析时误拦 51 条 stub。
- **Feature (LHT):** `gofmt -l` 跨平台原生探针（`verify_platform`），Windows 无需 bash 即可作格式门。
- **Config:** `~/.zagens/config.toml` 示例增加 `completion_gate.mode = enforce` + 全局 `gofmt` verify；MicroStack 层3 夹具仍见 `docs/harness/fixtures/microstack-completion-gate.toml`。

### Zagens desktop — LHT Phase 4 宏观审查循环

- **Feature (LHT Phase 4 — macro review loop):** 微观完成门通过后，可选 **LHT 实现段 → CRAFT Review → 补全段** 有界宏观循环（`[long_horizon.macro_loop]`，默认关）。编排：`macro_loop.rs`（blockers→checklist 幂等转换、段状态机）；micro `graph_complete` 后评估；harness 程序化 spawn CRAFT Review（`spawn_macro_craft_review`）；遥测 `long_horizon.macro_phase` / `macro_craft_start` / `macro_craft_result` / `macro_unmet`。**用户确认**默认：`/lht-craft-go` 或「开始审查」进入审查轮。Desktop **LHT 配置面板**新增「宏观审查循环」区（非 Composer 第四态；需 strict）。**LongHorizonPanel** Nodes/任务图展示宏观段摘要与 `macro_*` 节点着色；**Harness 预置**（`lht_presets.rs` + `apply_lht_preset`）：`long-refactor` 自动 `strict` + `macro_loop.enabled` + `on_micro_pass`。Files: `macro_loop_panel.rs`, `LongHorizonPanel.tsx`, `lht_presets.rs`, `commands.rs`, i18n ×4.

### Zagens desktop — 工作台预览

- **Feature (HTML 预览模式):** 工作台打开 `.html` / `.htm` 时可在「代码」与「预览」（iframe 渲染）之间切换；Office 侧车 `.preview.html` 默认进入预览。Files: `preview/detector.ts`, `preview/renderers/HtmlRenderer.tsx`, `PreviewDispatcher.tsx`.

### Runtime / prompts

- **Feature (`web_search` 多后端):** `web_search` 工具新增 5 个可配置搜索后端，满足国内访问需求。在 `~/.deepseek/config.toml` 中配置 `[search]` 表即可切换：
  - `metaso`（秘塔搜索）— 国内友好，有内置社区免费 Key，无需配置即可尝试
  - `baidu`（百度 AI 搜索 / 千帆）— 需 `BAIDU_SEARCH_API_KEY` 或 `api_key`
  - `bocha`（博查）— 国内 AI 搜索 API
  - `volcengine`（火山引擎 Ark）— ByteDance，带重试与 90 s 超时保护
  - `tavily` — 国际 AI 搜索 API
  - `bing` — 可单独指定必应（不再仅作 DDG 的 fallback）
  - 默认仍为 `duckduckgo`（自动 Bing fallback），行为不变。
  API Key 支持 config 文件 `api_key = "..."` 或对应环境变量（`METASO_API_KEY`、`BAIDU_SEARCH_API_KEY`、`VOLCENGINE_API_KEY` 等）。
  Files: `tools/web_search.rs`, `config/types.rs`, `tools/spec.rs`, `core/engine/types.rs`, `core/engine/tool_context.rs`, `runtime_threads/engine_spawn.rs`.

- **Fix (sub-agents):** `[subagents] heartbeat_timeout_secs` (default 300s) — maintenance loop auto-cancels running sub-agents with no progress and releases concurrent slots (upstream v0.8.52 parity).
- **Fix (snapshots):** `[snapshots] max_workspace_gb` (default 2) — skip side-git init / `git add -A` when the workspace tree exceeds the cap; prevents first-turn hangs on huge trees.
- **Fix (LLM errors):** HTTP error bodies are sanitized (HTML stripped, secrets redacted, length capped) before UI/logs/model context — avoids Cloudflare HTML dumps and leaked tokens.
- **Feature (tool approval):** Session-scoped approval cache wired into the engine; `write_file` / `edit_file` / `write_office` fingerprint by path; desktop dialog adds「本会话记住」→ `POST resolve-approval` `remember_for_session`.
- **Fix (MCP):** SSE / Streamable HTTP clients honor `HTTP(S)_PROXY` and `NO_PROXY` (reqwest has no default proxy in this workspace).


### Zagens desktop — 审计方格会话隔离

- **Fix (审计方格子代理面板):** 新建会话或切换历史会话时清空子代理面板状态，避免上一会话的 Completed 卡片泄漏到新 thread。Files: `useSessionNavigation.ts`, `App.tsx`.

### Docs

- **Desktop (办公场景地图 — 落地口径同步):** [`docs/desktop/OFFICE_SCENARIOS.md`](docs/desktop/OFFICE_SCENARIOS.md) — Phase A ~90%、11 技能/卡片、P0 落地状态列；扫描件 OCR 标为视觉桥接（`describe_image`）；STT/TTS、ERP/CRM、`inbox`/`data` 自动初始化标为本期范围外；§8 能力差距与附录 A 状态对齐。
- **Docs (架构边界分析):** 新增 [`docs/tech/ARCHITECTURE_BOUNDARY_ANALYSIS.md`](docs/tech/ARCHITECTURE_BOUNDARY_ANALYSIS.md) — sidecar 三通道连接、硬/软边界、场景化评估与速查矩阵。

### MCP 超时改善

- **Fix (MCP connect_timeout 默认值):** 将 `connect_timeout` 默认值从 10 秒提升至 **30 秒**，解决 `npx -y` 类 stdio 服务器在首次下载包时因超时导致连接时有时无的问题。Files: `crates/runtime-adapters/src/mcp/config.rs`.
- **Feature (MCP 编辑面板超时字段):** MCP 服务器编辑对话框新增 **连接超时 / 执行超时 / 读取超时** 三个数字输入框，供用户按需覆盖全局默认值。Files: `McpPanel.tsx`, i18n ×4.

### Zagens desktop — Office P0 实施（Phase A Step 1–2）

- **Feature (办公 P0 空态卡片):** 办公空态新增 **经营日报汇总**、**客户报价单**、**生产品质晨报** 任务卡片，prefill 对齐对应 `office-*` 技能。Files: `OfficeEmptyState.tsx`, i18n ×4.
- **Feature (办公 bundled 技能 v8):** `install_system_skills` 纳入 `office-executive-daily-brief`、`office-customer-quote`、`office-production-daily-report`（bundled marker **v8**）。Files: `skills/system.rs`, `assets/skills/office-*`.
- **Docs (技能契约):** 现有 8 个 `office-*` 技能统一补 `## 技能契约` YAML 块（约定层，引擎不解析）。
- **Docs (P0 fixtures):** `office-demo/data/` 价目表 CSV、客户需求、**生产日报_昨日.xlsx**（`scripts/gen-office-demo-fixtures.py` 可重生）。
- **Tooling:** `scripts/office-demo-oracle.ps1` — P0-2 / P0-3 / P0-4 deliverables 验收 oracle。

### Docs

- **Desktop (办公场景地图):** 新增并迭代 [`docs/desktop/OFFICE_SCENARIOS.md`](docs/desktop/OFFICE_SCENARIOS.md) — 四轴架构、L1–L4 分层、技能契约三阶段落地、§4 四轴缩写列、P0 探针说明；样板技能 `office-executive-daily-brief`；演示 fixtures [`docs/harness/fixtures/office-demo/`](docs/harness/fixtures/office-demo/README.md)。
- **Desktop (办公统一架构):** `OFFICE_SCENARIOS.md` 新增「四轴正交模型」（摄取/处理/输出/交互，§2.3）与 4 层统一架构 + 声明式「技能契约」（§3.1/§3.2），将 40+ 场景收敛为「同一流水线 × 四轴取值」，新增场景退化为填表；P0-1~P0-4 标注为四轴验证、§8 能力差距挂回 L2 原语。
- **Harness (DEMO7):** 新增 DEMO6 超集长程规格——目标 ≥10k 行 Go（`loc_gate.sh`）、class/while、fmt/lint/disasm、testdata≥50。File: `docs/harness/test-cases/DEMO7-monkey-platform-10k.md`.
- **Harness (DEMO6):** 新增 DEMO3 超集对比规格——Monkey 双后端（tree + 字节码 VM）、`parity.sh` / `coverage_gate.sh`、Zagens vs Cursor 同一 oracle 记录表。File: `docs/harness/test-cases/DEMO6-monkey-dual-backend.md`.
- **Harness (DEMO3):** 补 §8——`F:\DEMO3`（`thr_e2c4` 线程导出）产物 oracle 真绿 + B/`verify_mismatch_nudge` 闭环记录。File: `docs/harness/test-cases/DEMO3-monkey-interpreter.md`.

### Runtime

- **Fix (MCP 面板误报未连接):** `GET /v1/apps/mcp/servers` 与 `/tools` 改为读取 sidecar **共享 `McpPool`** 的实时连接状态，不再每次新建临时池并用 2s 超时判定（Agent 已连通时面板仍显示「未连接」）。Files: `runtime_api/mcp.rs`, `McpPanel.tsx`.
- **Fix (MCP stdio Windows 启动失败):** 新增 `mcp/stdio_spawn.rs`：为 sidecar 子进程补全 `PATH`（Node/npm 常见目录），解析 `npx`→`npx.cmd`，并通过 `cmd.exe /C` 执行批处理 shim，修复 GUI 启动时 `MCP stdio spawn failed` 无法连接 npm 类服务器的问题。Files: `stdio_spawn.rs`, `connection.rs`.
- **Feature (MCP UI 与可观测性):** 新增 `mcp/observability.rs` 内存环形调用日志；`connection`/`pool` 记录 RPC 与工具调用耗时；`GET /v1/apps/mcp/discover`（tools/resources/prompts 快照 + 最近调用）与 `GET /v1/apps/mcp/calls`。桌面 `McpPanel` 服务器详情改为标签页（工具逐条开关、资源、提示词、调用记录），每 20s 轮询连接状态。`crates/mcp` 增加弃用说明 README。Files: `observability.rs`, `config_io.rs`, `runtime_api/mcp.rs`, `McpServerDetail.tsx`, `McpPanel.tsx`, `crates/mcp/README.md`. (MCP 迭代方案阶段 4)
- **Feature (MCP 配置热重载):** sidecar 启动时创建进程级共享 `McpPool`，所有 Engine 复用同一连接池；新增 `POST /v1/apps/mcp/reload` 与 `McpPool::reload_config`（diff 配置 → 断开移除/变更项 → 重连）。桌面 `McpPanel` 在增删改/合并后自动热重载，并提供「应用配置」按钮，替代原先保存后必须重启 sidecar 的流程。Files: `crates/runtime-adapters/src/mcp/pool.rs`, `crates/runtime-server/src/mcp_shared.rs`, `runtime_serve/http.rs`, `runtime_api/mcp.rs`, `tool_context.rs`, `web-ui/McpPanel.tsx`, `api/client.ts`. (MCP 迭代方案阶段 3，后台健康探测留待后续)
- **Feature (MCP 远程认证 — 静态头):** 远程 MCP（`sse`/`http`）支持 `headers` 与 `auth`（`bearer` / `apiKey`）配置；值支持 `${ENV_VAR}` / `$VAR` 环境变量占位，连接时解析并注入 reqwest 默认头。`GET /v1/apps/mcp/servers/{name}` 对明文密钥脱敏（省略敏感字段），`PUT` 保存时 [`merge_preserved_secrets`](crates/runtime-adapters/src/mcp/auth.rs) 合并未改动的旧密钥，避免 UI 回写清空。桌面 `McpPanel` 编辑对话框增加 headers / auth 字段。OAuth 留待后续。Files: `mcp/auth.rs`, `config.rs`, `connection.rs`, `config_io.rs`, `runtime_api/mcp.rs`, `web-ui/McpPanel.tsx`, `types/mcp.ts`. (MCP 迭代方案阶段 2，静态头部分)
- **Feature (MCP Streamable HTTP 传输):** 新增 `StreamableHttpTransport`（单 endpoint POST JSON-RPC + `application/json`/`text/event-stream` 响应解析 + `Mcp-Session-Id` 会话维护 + `MCP-Protocol-Version` 头），可连接仅提供 2025 规范 Streamable HTTP 的远程 MCP 服务器。`connection.rs` 按 `transport_kind`（`stdio`/`sse`/`http`）分派，`call_method` 将整个 send+recv 纳入超时以正确约束 HTTP 长调用。Files: `crates/runtime-adapters/src/mcp/transport.rs`, `connection.rs`, `config.rs`. (MCP 迭代方案阶段 1)
- **Feature (MCP 协议版本协商):** `initialize` 改为通告 `2025-06-18` 并解析服务器返回的 `protocolVersion` 做协商/降级（支持 `2025-06-18`/`2025-03-26`/`2024-11-05`，未知版本告警后尽力兼容），协商结果下发给传输层。Files: `crates/runtime-adapters/src/mcp/connection.rs`.
- **Feature (MCP 传输配置 + 类型同步):** `McpServerConfig` 新增 `transport`（别名 `type`，可选 `stdio`/`sse`/`http`，省略时按 command/url 推断、url 默认 SSE 向后兼容）；快照与 `GET /v1/apps/mcp/servers` 回填解析后的 transport 标签；web-ui `McpServerConfigPayload` 同步 `transport` 字段，服务器列表徽标优先展示后端解析值。修复 `list_mcp_tools` 复用 `split_once('_')` 的同名缺陷（改用已知 `tool.name` 反推 server）。Files: `crates/runtime-adapters/src/mcp/config.rs`, `config_io.rs`, `crates/runtime-server/src/runtime_api/mcp.rs`, `crates/desktop/web-ui/src/types/mcp.ts`, `components/McpPanel.tsx`. (MCP 迭代方案阶段 1)
- **Fix (MCP 工具 `isError` 误判为成功):** MCP `tools/call` 执行路径改用 `extract_tool_content()` 提取 `content[]` 文本块（非 text 块降级为 `[<type> content]` 占位符），并按 MCP 规范将 `isError == true` 映射为 `ToolResult::error`，不再把含 `content`/`isError`/`meta` 的原始 JSON 噪声当成功结果塞给模型。Files: `crates/runtime-server/src/core/engine/tool_execution/mcp.rs`, `crates/runtime-adapters/src/mcp/format.rs`, `mcp/mod.rs`. (MCP 迭代方案阶段 0)
- **Fix (MCP 含下划线服务器名工具调用失败):** `pool.rs::parse_prefixed_name` 改为按已配置服务器名最长前缀匹配解析 `mcp_{server}_{tool}`，与拼接逻辑对称；含下划线的服务器名（如 `github_mcp`）不再被错误拆分，未知服务器名回退首下划线切分。补含下划线服务器名、`isError=true`、非 text content 块的单元测试。Files: `crates/runtime-adapters/src/mcp/pool.rs`, `mcp/tests.inc.rs`. (MCP 迭代方案阶段 0)
- **Feature (办公数据管道):** `write_office` XLSX `sheets[].source` 直喂 CSV/TSV/XLSX（免模型重抄整表）；`read_office` 从 PPTX slide 关系读取**图表系列数据**。Files: `office_common.rs`, `office_write.rs`, `office_read.rs`, `office.md`, `office_smoke.rs`.
- **Feature (办公产品化 P2):** 8 个 `office-*` 技能纳入 `install_system_skills`（bundled marker **v6**，含 7 个新增 `SKILL.md` 资产）；办公 tool surface 增加 `describe_image`（扫描件 OCR）；`office_smoke` 增加 numFmt golden 与旧版 `.doc` 提示测试。Files: `assets/skills/office-*`, `skills/system.rs`, `registry.rs`, `office_smoke.rs`, `tool_catalog.rs`, `base-office.md`.
- **Fix (办公 `read_office` 首轮不可见):** `read_office` / `load_office_payload` 与 `write_office` 一样在 Agent 模式**预加载**（不再默认 `defer_loading`），避免模型首轮只见 `read_file`、对 XLSX 只抽到 sheet 名。`base-office.md` 工具表改为优先 `read_office`。Files: `crates/core/src/engine/tool_catalog.rs`, `prompts/base-office.md`.

### Zagens desktop

- **UX (MCP 添加简化):** 移除分栏「快速添加」表单，统一为单一 **「添加 MCP」** JSON 粘贴（与 Cursor/Claude 相同格式）；无服务器时自动展开编辑区；示例改为 `server-everything`。Files: `McpPanel.tsx`, i18n ×4.
- **UX (toast 位置):** 全局 toast 从输入区上方居中改为窗口**右下角**堆叠；Composer 内联错误/转写状态也改为 toast，输入框不再被提示挤占。Files: `toast.tsx`, `Composer.tsx`.
- **Fix (启动慢 / 控制台 401 风暴):** 工作区文件树 `useEffect` 误将每次渲染新建的 `treeCache` 对象列入依赖，办公模式反复请求 `deliverables` browse；所有 runtime HTTP 在 `initRuntimeConfig`（Tauri 端口 + Bearer 代理）完成前排队等待，避免 sidecar 未就绪时裸连 `127.0.0.1:7878` 触发海量 401。Files: `WorkspaceFilesPanel.tsx`, `web-ui/src/api/client.ts`.
- **Fix (启动卡死 / `plugin:event|listen` 循环):** `App` 每帧传入新的 `onCancelSideEffects` 导致 `handleCancelStream` 与 sidecar `listen`/`unlisten` 在每次渲染时重建（网络面板可达数千次）；改为稳定 `useCallback`，sidecar 订阅改经 ref 绑定。Files: `App.tsx`, `useTurnStream.ts`, `useTurnStreamRecovery.ts`.

- **Feature (办公空态 8 卡):** 空态扩展为 8 个任务卡片（周报、纪要、汇报 PPT、数据报表、竞品分析、合同初稿、简历、发布说明），prefill 对齐 `load_skill office-*`。Files: `OfficeEmptyState.tsx`, i18n ×4.
- **Feature (磁盘压力):** 监测 `~/.zagens` 与当前工作区所在盘剩余空间；临界（&lt;100MB）时自动 **停止** 进行中回合、禁止新发消息，并显示顶部告警（针对 DEMO8 观察：C 盘满 → WebView「页面不存在」且刷新后 LHT/计划仍在后台跑、继续扣费）。`index.html` 增加脚本加载失败时的静态说明。Harness 记录见 `DEMO8-monkey-blind-goal-only.md` §3.5。Files: `disk_guard.rs`, `get_storage_pressure`, `useStoragePressure.ts`, `StoragePressureBanner.tsx`, `ShellLoadFailure.tsx`.
- **Fix (流式断连恢复):** Sidecar 重启或 runtime 离线时，不再把 UI 误判为「已停止」而后台继续扣费——保持流式锁定、持久提示 + **停止**、在 `sidecar://ready` / 探测恢复后自动重连 SSE；离线超过 2 分钟自动 `interrupt` 后台回合。Files: `useTurnStreamRecovery.ts`, `useTurnSend.ts`, `useTurnStream.ts`, `useDesktopShell.ts`.
- **Fix (聊天流与面板 desync):** 重连已有回合时重新绑定最后一条 assistant 气泡，恢复思考链/工具链/正文的 SSE 增量；`finishOnce` 在后台回合仍活跃时不解锁；无 live handler 时每 8s 从线程事件回放刷新聊天（右栏 checklist/LHT 仍靠 HTTP 轮询）。Files: `activeTurnStreamUi.ts`, `useTurnSend.ts`, `useTurnStreamRecovery.ts`.
- **Docs (harness DEMO7):** §4.4 Zagens `F:\DEMO6-3` 人工 oracle 记录（11/12、`loc_gate` 10014、vm coverage 7.9%）；§5 对比表预填。File: `docs/harness/test-cases/DEMO7-monkey-platform-10k.md`.
- **Docs (harness):** DEMO7 §8 — 子代理并行铺 examples/testdata/测试以缩短长程墙钟，作为后续 LHT/prompt 迭代方向（对照 OpenCode）。File: `docs/harness/test-cases/DEMO7-monkey-platform-10k.md`.
- **Docs (harness DEMO7):** §4.5 OpenCode `F:\DEMO6-5` oracle **12/12**、墙钟 **64 min**、对话与 oracle 一致；与 Zagens 11/12 对照。File: `docs/harness/test-cases/DEMO7-monkey-platform-10k.md`.
- **Docs (harness DEMO8):** 新增盲测规格 — 仅目标 prompt、隐藏 DEMO7 §4 oracle、Zagens LHT Off/Strict 两档、三方记录表。File: `docs/harness/test-cases/DEMO8-monkey-blind-goal-only.md`；索引 `LHT_TEST_SUITE.md`.
- **Docs (harness DEMO8):** 实盘路径 `F:\DEMO6-6|7|8`（Cursor/Zagens/OpenCode）与开工快照记入 §3.4。
- **Docs (harness DEMO8):** §3.6 收工状态 — Zagens `F:\DEMO6-7` 已结束；Cursor 中断；OpenCode 进行中（待 §4 oracle）。
- **Docs (harness DEMO8):** §4.4 Zagens 盲测收工 UI 证据（checklist/plan/manifest 全绿 vs 产物 ~4.3k 行、无 scripts）。
- **Docs (harness DEMO8):** §3.7 右栏可观测性 — OpenCode Checklist 非实时；Zagens 实时更新。
- **Docs (harness DEMO8):** §4.5 OpenCode `F:\DEMO6-8` 盲测收工摘要（15 sub-tasks · 与 Zagens 扫盘对照）；§3.6 三方终态更新。
- **Docs (harness DEMO8):** §3.8 C: 耗尽与 F: 工作区分裂 — 解释 Zagens bat/终态扫盘与 §4.4 时间差、OpenCode 相对未受影响。
- **Docs (harness DEMO8):** §4.6 Zagens 第二轮 bat 自验 vs DEMO7 §4 官方复跑 **4/12**（`F:\DEMO6-7`）。
- **Feature (办公空态入口):** 办公会话无消息时展示 4 个任务卡片（周报、纪要、汇报 PPT、数据报表），点击填充 Composer 提示（含 `load_skill office-weekly-report`）。Files: `OfficeEmptyState.tsx`, `ChatView.tsx`, i18n ×4.
- **Docs:** `office-mode-iteration-plan.md` 同步预览策略（PDF/HTML 右栏；Office 系统打开）与推荐顺序实施状态表。

- **Feature (办公预览与发现):** 右侧内嵌预览 **PDF / HTML**；**DOCX/PPTX/XLSX** 双击或生成后用系统默认应用打开；`write_office` 完成后刷新 `deliverables` 并按扩展名分流。Files: `openWorkspaceSystem.ts`, `PdfRenderer.tsx`, `HtmlPreviewRenderer.tsx`, `useWorkspacePanel.ts`.
- **Enhancement (办公 UI):** Composer 办公状态条；设置页 Office 环境就绪状态。Files: `Composer.tsx`, `SettingsPanel.tsx`, i18n ×4.

---

## [0.6.1-preview.1] - 2026-06-02

### Zagens desktop

- **Fix (话题记忆图 — 边筛选对齐引擎):** `selectHotTopicSubgraph` 原错误要求边两端均在热节点集合内，与 Rust `generate_memory_section` 全图 Top-6 边策略不一致，导致注入 Markdown 中有关联而图上不显示。修正为先全局按权重取 Top 6 边，再由 `buildTopicMemoryLinks` 在绘制时过滤两端不在画布的边，与引擎行为完全对齐。File: `topicMemoryGraphLayout.ts`。
- **Enhancement (话题记忆图 — 实时刷新):** 流式回合结束后立即触发一次 `refresh()`（`prevStreamingRef` + effect），不再需等待 15 秒轮询。File: `TopicMemoryPanel.tsx`。
- **Enhancement (话题记忆图 — 列表/图联动):** 列表中点击不在热子图内的话题节点时，显示「不在图中」标注，消除视觉歧义。File: `TopicMemoryPanel.tsx`，i18n ×4（新增 `notInGraph`）。
- **Enhancement (话题记忆图 — 加载与错误态):** 首次加载前显示骨架屏（6 格指标 + 图区占位）；请求失败且已有旧数据时显示「数据可能已过时」琥珀色警告（保留旧快照而非清空），无旧数据时显示红色错误信息。File: `TopicMemoryPanel.tsx`，i18n ×4（新增 `graphStale`）。
- **Enhancement (话题记忆图 — 边可视化):** 边增加 SVG `<title>` tooltip 显示 `A → B  (weight)`；图区左下角新增「细线 → 粗线 = 弱 → 强关联」图例。File: `TopicMemoryGraphSvg.tsx`。
- **Perf (话题记忆图 — 渲染):** 边循环中 `layout.find`（O(n) 遍历）改为从 `posById` Map 直接 O(1) 查找节点半径，消除不必要遍历。File: `TopicMemoryGraphSvg.tsx`。
- **i18n (日语 settings — 话题记忆):** `settings.topicMemory` 和 `settings.topicMemoryInterval` 补充日语翻译（此前仍为英文占位）。File: `ja.ts`。

---

### Zagens desktop

- **Enhancement (话题记忆图面板):** 网络图仅展示引擎同策略的 Top 12 话题 / Top 6 关联；滚轮缩放、拖拽平移、节点 hover/列表联动高亮、连线端点避让；补充常见关联、知识边界、认知轨迹与空态文案；指标增加重复话题率、每 10 回合注入。Files: `TopicMemoryPanel.tsx`, `TopicMemoryGraphSvg.tsx`, `topicMemoryGraphLayout.ts`, `client.ts`, i18n ×4。
- **Fix (话题记忆图可视化):** 边键按引擎格式 `A→B` 解析（此前误用 `->` 导致连线不显示）；力导向布局、节点标签与边粗细/透明度按权重展示，对齐 `docs/topic-memory-graph-main` 认知地图风格。Files: `TopicMemoryGraphSvg.tsx`, `topicMemoryGraphLayout.ts`, `TopicMemoryPanel.tsx`。
- **Fix (关于页外链):** 官网与「官网手动下载」改用 `open_external_url` 在系统浏览器/邮件客户端打开（Tauri WebView 内 `<a href>` / `window.open` 无效）。
- **Feature (OTA 应用内更新):** 接入 `tauri-plugin-updater`（`get_update_status` / `install_app_update`、关于页检查/安装、启动 toast）；`createUpdaterArtifacts` + 仓库公钥；[`docs/desktop/UPDATER.md`](docs/desktop/UPDATER.md)；`website/scripts/sync-download-manifest.mjs` 写入 `latest.json` 的 NSIS 签名与 setup.exe URL。CI Release 支持 `TAURI_SIGNING_PRIVATE_KEY`。
- **About（关于）:** 产品描述与 [官网](https://zagens.com/) 文案对齐；新增支持邮箱 `didclawapp@gmail.com` 与官网链接。Files: `AboutPanel.tsx`、i18n ×4。

### Zagens desktop

- **Fix (UI 文案 — 用户数据路径 ~/.deepseek → ~/.zagens):** MCP、会话、API Key、skills 等 i18n 四语言路径统一为 `~/.zagens/`；runtime MCP merge 错误提示同步。Files: `web-ui/src/i18n/locales/*`, `types/desktop.ts`, `api/client.ts`, `runtime-adapters/.../config_io.rs`。
- **Fix (UI 文案 — 移除已 sunset 的 CLI/TUI 引用):** 设置/API Key、快照恢复、技能面板、系统设置等 i18n 四语言去掉「CLI/TUI 共用配置」「TUI /restore」「终端 TUI /skill install」等过时表述，改为 desktop + `~/.zagens` + runtime sidecar。Files: `web-ui/src/i18n/locales/{zh-Hans,en,ja,pt-BR}.ts`。

### Runtime

- **Skills (随包安装):** 新增 bundled **`multi-search-engine`**（16 引擎多源搜索，`SKILL.md` + `config.json` + `references/*`）；bundled skills marker **v5**。Files: `crates/runtime-server/assets/skills/multi-search-engine/`, `crates/runtime-server/src/skills/system.rs`。

- **Feature (Composer LHT 三态开关 — auto / strict / off):** Composer 顶栏由二态 boolean 改为循环三态（`LHT` → `LHT·严格` → `LHT·关`）；`settings.toml` 新字段 `lht_composer_mode`（legacy `lht_strict` 迁移为 strict/auto）。**off** 在 engine spawn 硬设 `long_horizon.enabled=false`；**strict** 强制 enforce；**auto** 继承 `config.toml`。Tauri `get/set_lht_composer_mode`；LHT 配置面板显示 Composer 覆盖提示。Files: `crates/config/src/ui_settings.rs`、`engine_spawn.rs`、`LhtModeToggle.tsx`、`LhtSettingsPanel.tsx`、i18n ×4。

- **Tooling (LHT harness E2E tests — Phase 0–1):** 新增 headless 端到端测试：`scripts/lht-harness-smoke.ps1`、`scripts/lht-harness-run.ps1`、`scripts/lht-harness-report.py`、`scripts/lht_harness_util.py`、`scripts/lht-harness-lib.ps1`；修正 `runtime-longrun-baseline.ps1` 为 `zagens-runtime --port --config`。新增 **strict 任务集** [`docs/harness/fixtures/lht-harness-tasks.strict.toml`](docs/harness/fixtures/lht-harness-tasks.strict.toml) + 种子 [`docs/harness/fixtures/strict-task-seed/`](docs/harness/fixtures/strict-task-seed/)。规格 [`docs/harness/LHT_EVAL_INFRASTRUCTURE.md`](docs/harness/LHT_EVAL_INFRASTRUCTURE.md)。

- **Fix (LHT 层2 verify — Windows 上 `grep`/`rg`/`test -d` 原生探测，消除 infra 假 RED):** `label_rust` Round 2 实证：checklist 项 `[verify: grep -c not_impl …]` 在 Windows PowerShell 下因无 `grep` 被判 `exit_class: infra`，manifest 8 轮耗尽 (`manifest_rounds_exhausted`) 而代码已达标。新增 `verify_platform.rs`：manifest 门在执行 shell 前对常见 Unix 探测（`grep`/`rg` 计数或匹配、`test -d`/`test ! -d`）做**跨平台 in-process** 扫描；对 `not_impl`/`todo!`/`unimplemented` 等 stub 模式采用 **absence 语义**（零匹配 = 通过）。`verification_satisfied` 扩展等价 normalized 形式。Fixture：[`lht-label-rust-round2-checklist.md`](docs/harness/fixtures/lht-label-rust-round2-checklist.md)、[`lht-refactor-round2-checklist.md`](docs/harness/fixtures/lht-refactor-round2-checklist.md)。Files: `crates/runtime-server/src/long_horizon/{verify_platform.rs,manifest_gate.rs,verify.rs,mod.rs}`。

### Docs

- **Docs (LHT 端到端测试基建 v0.2):** 重写 [`docs/harness/LHT_EVAL_INFRASTRUCTURE.md`](docs/harness/LHT_EVAL_INFRASTRUCTURE.md) 定位 —— **Harness 正规 L2 测试方法**（三层金字塔、oracle + harness 不变量、profile 对照、outcome 分诊、PR/nightly gate）；非论文统计一等目标。夹具 [`fixtures/lht-eval-tasks.example.toml`](docs/harness/fixtures/lht-eval-tasks.example.toml)、[`fixtures/lht-eval-arms/`](docs/harness/fixtures/lht-eval-arms/)（规划 rename 为 `lht-harness-*`）。

- **Docs (LHT Round 2 — label_rust Tauri 补全清单):** 新增 [`docs/harness/fixtures/lht-label-rust-round2-checklist.md`](docs/harness/fixtures/lht-label-rust-round2-checklist.md)（Round 1 后 43× `not_impl`、adapters/sync 接线、`npm run build` / `cargo tauri build` 验收；含可复制开场指令 + 17 项 `[verify:]` checklist）；[`lht-refactor-round2-checklist.md`](docs/harness/fixtures/lht-refactor-round2-checklist.md) 交叉引用。

- **Docs (论文草稿 — LabelMakePro 自发宏流程实证):** [`docs/harness/PAPER_silent_early_stopping.md`](docs/harness/PAPER_silent_early_stopping.md) 升 v0.2：新增 **§7.6**（`F:\label_rust` LabelMakePro v2.67.1 · LHT·strict + CRAFT Explore + audit scratchpad · 16/16 · manifest 两轮自愈 · 800 元 vs Sonnet ~$11k–15k 成本对照）、**§8.5**（strict 硬门禁产品含义）、**附录 C**（`thr_3658ee8d` gate 时间线与交付物路径）。

- **Docs (LHT 产品迭代路线图 P0–P3):** 在 [`docs/harness/LONG_HORIZON_CODE_TASKS.md`](docs/harness/LONG_HORIZON_CODE_TASKS.md) 新增 **§6 产品迭代路线图**（2026-06 · 大 refactor 35min 压测驱动）：完成度诚实预期表、实证摘要、**P0** strict 全 enforce + mismatch 阻断 + UI 有条件完成、**P1** 细 checklist / IPC manifest / 工具链感知 / 跨层验收、**P2** Phase 4 + 缺口枚举器、**P3** 测量与金矿 backlog；实施状态表增 P0–P3 行；§8/§9/§11/§15.6 同步；金矿 backlog 行并入 P3-5。

- **Docs (LHT Phase 4 — LHT↔CRAFT 组合式宏观循环规格):** 在 [`docs/harness/LONG_HORIZON_CODE_TASKS.md`](docs/harness/LONG_HORIZON_CODE_TASKS.md) 将 Phase 4 从「可选 CRAFT 末段」扩展为 **LHT 实现段 → CRAFT 质检段 → LHT 补全段** 的有界宏观循环（`max_macro_cycles`、blockers→checklist 编排、CRAFT 作缺口枚举器非法官、与 Composable 层2/3 micro 闭环 compose）；新增 §7.4 大 refactor 走查、§15.5 实施 PR 草案。交叉引用：[`COMPOSABLE_HARNESS.md`](docs/harness/COMPOSABLE_HARNESS.md) §4 macro 第四维、[`harness/README.md`](docs/harness/README.md) 索引更新。动机：单次 LHT realistic ~70–80%，宏观 1–2 轮目标 ~85–90%+（label_rust 类迁移实测与设计对话）。

### Zagens desktop

- **Feature (Composer LHT 三态):** 顶栏 `LhtModeToggle` 单击循环 **LHT → LHT·严格 → LHT·关**；`settings.toml` 字段 `lht_composer_mode`。`LhtSettingsPanel` 在 off/strict 时显示 Composer 覆盖提示并灰显被覆盖项。

- **Fix (Zagens 启动页):** 连接等待阶段改为全屏 `bg-canvas`，仅显示「启动中，请稍等」；移除欢迎语、进度条与连接状态文案。API Key / 模式步骤 UI 不变。 首次创建 `~/.zagens/` 时写入 `config.advanced.example.toml`（OpenRouter / OpenAI / Ollama / vLLM / SGLang 常用模型 id、profiles 示例；**不自动加载**、UI 不展示）。复制片段到 `config.toml` 后重启 sidecar。仓库 `config.example.toml` 同步补充 `[providers.openrouter]` / `[providers.openai]`。

- **Fix (工作区元数据目录 `.deepseek/` → `.zagens/`):** 每个工作区根目录下的规则、审计 scratchpad、符号索引、CRAFT blackboard、子 Agent 状态、handoff、项目级 `config.toml` 等元数据**新写入**统一落到 `{workspace}/.zagens/`；读取仍优先 `.zagens/`，不存在时回退 legacy `{workspace}/.deepseek/`。`~/.deepseek/` 用户主目录与 provider/API 命名不变。

### Docs

- **Docs (LHT 方案文档对齐仓库):** [`LONG_HORIZON_CODE_TASKS.md`](docs/harness/LONG_HORIZON_CODE_TASKS.md) 修订 P0/P1/P1′ 为「已落地」、更新 §3.3.1 可视化现状（`LongHorizonPanel` / `LhtModeToggle` / `LhtSettingsPanel`）、`.zagens/handoff.md` 路径、实证摘要加「修复列」、`strict_completion_gate` 子门须已 `on` 的准确语义、Phase 2/3 状态与迭代 mermaid。

### Runtime

- **Feature (LHT 产品迭代 P1′ — 80% 路径三件套):** `label_rust` 第二轮压测暴露：单体 `lib.rs` IPC 未进层3 manifest、`electron/` 仍在但 shim 误报 observe、`cargo test` 0 tests 空绿、enforce 下 UI 仍纯绿。**P1c+** `merge_runtime_deliverables` 扫描 `#[tauri::command]` in `lib.rs` + 迁移必备 deliverable（`tauri.conf.json`、adapter glob）。**integration′** shim-aware 跨层门：`electron/` 存在 → strict enforce reinject（`NudgeIntegrationIncomplete`）；有 shim 时不再数 `electronAPI` 误报。**P0-3+** UI「有条件完成」在 `first_gap_count>0` 或 `integration_gap_count>0` 时一律 amber（不限 observe）。**toolchain′** polyglot 用 `cargo build` 替代空 `cargo test`。Fixture：[`fixtures/lht-refactor-round2-checklist.md`](docs/harness/fixtures/lht-refactor-round2-checklist.md)。Files: `integration_gate.rs`、`deliverable_manifest.rs`、`completion_gate_flow.rs`、`generic_gate.rs`、`gate_telemetry.rs`、`nudge.rs`、`no_tool_uses.rs`、`completion_gate_panel.rs`、`LongHorizonPanel.tsx`、`i18n/locales/*.ts`。
- **Feature (LHT 产品迭代 P0 — strict 全 enforce + verify mismatch 阻断 + UI 有条件完成):** 大 refactor 压测（`label_rust`）暴露：加严模式只抬 `completion_gate.mode` + `stub_gate`，`auto_verify_replay` / `toolchain_gate` 仍 observe → checklist 100% 但 `first_gap_count>0` 仍结束 turn；item 5/6 曾 `verify_gate mismatch` 仍 completed。**P0-1** `strict_completion_gate()` 同步把已开启的 `auto_verify_replay`、`toolchain_gate` 提到 `enforce`。**P0-2** 在 `graph_complete` 旁路新增 `[verify:]` **mismatch** 有界 nudge（`MAX_VERIFY_MISMATCH_NUDGES=2`、`build_verify_mismatch_nudge`、`LhtGateOutcome::NudgeVerifyMismatch`、遥测 `long_horizon.verify_mismatch_nudge`），与 DEMO3 `unverified_acceptance`  guard 并列。**P0-3/4 Desktop** checklist 100% 但 manifest 失败或 observe 缺口 → Task 面板 amber「有条件完成」横幅 + 计时器 amber；composer LHT 加严 tooltip 说明覆盖 completion 子门。Files: `runtime-server/src/long_horizon/{mod.rs,nudge.rs}`、`turn_loop/host_impl/no_tool_uses.rs`、`web-ui/src/components/{LongHorizonPanel.tsx,LhtModeToggle.tsx}`、`i18n/locales/*.ts`。
- **Feature (LHT 产品迭代 P1 — 工具链感知 · IPC manifest · 跨层 observe · plan 一致性):** **P1a** 工具链门：`detect_toolchain_entries` 识别 npm+cargo polyglot（`src-tauri/Cargo.toml`）时优先 `cargo check/test`、跳过根目录 `npm test` false-RED。**P1b** `base.md` 新增 long-refactor / 栈迁移 checklist 纪律段。**P1c** 层3 runtime 合并：`merge_runtime_deliverables` 读取 `{workspace}/.zagens/lht-deliverables.toml` + 自动发现 `src-tauri/src/commands/*.rs`（示例 [`fixtures/lht-deliverables.example.toml`](docs/harness/fixtures/lht-deliverables.example.toml)）。**P1d** `integration_gate` observe 扫描 `electronAPI` vs `getDesktopAPI`/`invoke(`；`plan_drift` + `NudgePlanChecklistDrift` 阻断 plan 灰字 + checklist 全勾。Files: `generic_gate.rs`、`deliverable_manifest.rs`、`integration_gate.rs`、`plan_drift.rs`、`completion_gate_flow.rs`、`gate_telemetry.rs`、`base.md`。** 实测发现某次 LHT 跑里模型**没有任何 plan/checklist 就直接开干**,`sidecar.log` 反复出现 `long_horizon.gate_skip: {"reason":"graph_empty", ... "open_items":0}` —— `base.md` 只是**建议**先规划,门禁入口对「空任务图」没有任何**强制**,于是进度无法可视化、完成门/ stub 门因 `graph_empty` 全程被跳过,整张 LHT 网形同虚设。新增 `[long_horizon] mode`(`"auto"` 默认 | `"strict"`):**`auto`** 保持历史行为(模型自己发挥,空图直接 skip);**`strict`** 下当任务图为空但线程已有**实质工具活动**(`MIN_TOOL_USES_FOR_PLAN_GATE`=3 次 `tool_use`,据此排除一次性问答/过早触发)时,回灌**双语「先建计划」nudge**(`build_plan_required_nudge`,要求 `checklist_write` 拆出具体可验证步骤、复杂任务再 `update_plan`),由 `MAX_PLAN_GATE_ROUNDS`=3 兜底防空转(耗尽则诚实停、不无限 nudge);同时把完成门 / stub 门**自动提到 `enforce`**(`strict_completion_gate`),使「假完成」与「半成品」即便 operator 留在 `observe` 也被堵住——即「开启即必须按 LHT 走、无法避让」。新增遥测 `long_horizon.plan_gate`(`nudged`/`round`)与 `LhtGateOutcome::NudgePlanRequired` 独立节点。两种开启方式:**① 桌面 composer 顶部栏「LHT」开关(全局、免重启)** —— 新增自包含组件 `LhtModeToggle`,点按即经 `set_lht_strict` Tauri 命令把 `lht_strict` 写入 live 的 `settings.toml`(与 locale 同一通道),`engine_spawn` 每轮现读该值,命中即把 `long_horizon.mode` 抬到 `Strict` 并 `enabled=true`,故**下一轮即生效、无需重启 sidecar**;localStorage 镜像值用于即时绘制与浏览器开发态。**② 直接在 `~/.zagens/config.toml` 设 `[long_horizon] mode = "strict"`**(需重启 sidecar)。亦预留 per-turn 覆盖通道(`turn_lht_mode` / `lht_mode_override`,默认 `None` 不影响行为)备后续按会话开关。新增 4 个单测(tool_use 计数 / 工具太少不强制 / 实质工作强制后耗尽放手 / strict 抬升门禁模式)。Files: `crates/core/src/long_horizon.rs`(`LhtMode` + `mode` 字段 + toml 映射)、`crates/runtime-server/src/long_horizon/{mod.rs(`evaluate_plan_bootstrap`/`count_tool_uses`/`strict_completion_gate`+`NudgePlanRequired`+单测),nudge.rs(`build_plan_required_nudge`+`plan_gate_rounds`+常量),gate_telemetry.rs(`PlanGate` 事件)}`、`crates/runtime-server/src/core/engine/{runtime_ext.rs,build.rs,turn_loop/host_impl/no_tool_uses.rs}`、`crates/runtime-server/src/{settings.rs(`lht_strict` 字段),runtime_threads/engine_spawn.rs(live 覆盖)}`、`crates/config/src/ui_settings.rs(`read/write_lht_strict_setting`)`、`crates/desktop/src/{commands.rs(`get/set_lht_strict`),main.rs(注册)}`、`crates/desktop/web-ui/src/{components/{LhtModeToggle.tsx(新),Composer.tsx},i18n/locales/*.ts}`。
- **Feature (LHT 通用 stub/半成品门 — 任务无关地堵「编过但功能缺」的假完成):** 大型重构(如 Tauri 迁移)最常见的失败形态是**「项目能编译、`cargo build --release` exit 0、二进制也产出,但一堆功能其实还是 stub」** —— 绿色构建掩盖了缺失实现,门禁此前查不出。新增 `completion_gate` 的**任务无关第三类层**:在 `graph_complete` 时对工作区做一次**纯文件系统扫描**(无命令执行,故先于 layer-2/3 跑),命中**高信号「故意未完成」标记**即视为缺口 —— `todo!()` / `unimplemented!()`(Rust 宏,编过但运行即 panic)、`NotImplementedError` / `raise NotImplementedError`、以及 `throw`/`panic!`/`raise`/`return`/`reject` 携带 "not implemented" 的句子(语言无关)。`TODO`/`FIXME` 裸注释**仅记录、永不阻断**(真实代码里太常见,enforce 会误伤)。可配 `[long_horizon.completion_gate] stub_gate = "off"|"observe"|"enforce"`,**省略即 `observe`(先量后调)** —— 一旦 operator 启用了任何完成门禁,默认就把 stub 计数浮现到遥测,要静默需显式 `"off"`、要阻断设 `"enforce"`。`enforce` 命中阻断级标记即回灌强制返工的双语 nudge(`build_stubs_found_nudge`,列 file:line + 片段,上限 12 行),由 `max_manifest_rounds` 兜底防止对模型修不掉的 stub 空转;耗尽则诚实停(`stub_rounds_exhausted`)。扫描跳过 `node_modules`/`target`/`dist`/`.git` 等依赖与产物目录、仅看源码扩展名、按文件数/命中数/单文件字节封顶,跑在 `spawn_blocking`。不可信来源的 `enforce` 自动降级为 `observe`(不得凭 drive-by 配置阻断 turn)。新增遥测 `long_horizon.stub_gate`(mode + blocking/todo/total + 样本)与独立的 `LhtGateOutcome::NudgeStubsFound` 节点(与 verify 命令失败区分)。新增 4 个 scanner 单测(高信号阻断 / TODO 不阻断 / 跳过 deps+docs / 干净工程零命中)。Files: `crates/core/src/long_horizon/completion_gate.rs`(`stub_gate` 字段 + toml 映射 + `is_active`/`sanitized_for_source`)、`crates/runtime-server/src/long_horizon/{stub_gate.rs(新),completion_gate_flow.rs(`evaluate_stub_gate`),nudge.rs(`build_stubs_found_nudge`+`stub_gate_rounds`),gate_telemetry.rs(`StubGate` 事件),mod.rs(`NudgeStubsFound`)}`、`crates/runtime-server/src/core/engine/turn_loop/host_impl/no_tool_uses.rs`(处理 + 遥测)。
- **Fix (LHT 完成门禁在 Windows / 嵌套项目下的两个 false-RED — 实测 `F:\LHT_TEST\lable_Standalone` Tauri 迁移):** 一个 Tauri 迁移长程任务实际 `cargo build --release` 已 exit 0、二进制产出,但门禁判定"未绿",模型误诊为「F: 盘 / `\\?\` 路径基础设施限制」。`sidecar.log` 实证两个真实 bug:**① `task_gate_run` 工具 Windows 不可用** —— `crates/runtime-server/src/tools/tasks.rs` 的 `TaskGateRunTool` 硬编码 `Command::new("/bin/sh")`,Windows 无 `/bin/sh`,故每次(连 `echo hello`)都在 `duration_ms≈0` spawn 失败报 `os error 3`(ERROR_PATH_NOT_FOUND),与盘符/路径前缀无关。改为按平台选 shell(`gate_shell_command`:Windows `cmd /C`,其余 `/bin/sh -lc`),与 `exec_shell` 一致。**② 完成门禁对 polyglot/嵌套布局用错 cwd** —— `auto_verify_replay` 复跑模型声明的 `[verify: cargo check --lib]` 时,因 `resolve_project_root` 见根目录有 `package.json` 即返回根,而 Cargo 工程在 `src-tauri/`,于是 `cargo` 在根目录跑、报 `could not find Cargo.toml`(日志:`model_verify_5/6` enforce 失败 ×3 轮),纯属 cwd 错误的 false-RED(代码本身能编过)。新增 `generic_gate::resolve_command_root(workspace, command)`:按命令工具链标记(cargo→`Cargo.toml`、go→`go.mod`、npm/pnpm/yarn/node→`package.json`、pytest/python→`pyproject.toml`/`setup.py`、mvn→`pom.xml`、gradle→`build.gradle[.kts]`)做**逐命令** cwd 解析——根有标记用根,否则唯一 depth-1 子目录有标记则用之,歧义/无则回退根(是 `resolve_project_root` 的逐命令版,允许同一门禁里 npm 在根、cargo 在 `src-tauri/`)。`manifest_gate::run_single_verify` 改用该 per-command root。新增 4 个单测(含还原本案的 `command_root_descends_to_nested_cargo_under_npm_root`)。Files: `crates/runtime-server/src/tools/tasks.rs`、`crates/runtime-server/src/long_horizon/{generic_gate.rs,manifest_gate.rs}`。
- **Feature (LHT 一推到底 — 跨阶段连续推进 C1+C2):** 长程任务过去常在「完成一个阶段 → 写 handoff/总结 → 停下来等用户」处过早结束。两处闸门是根因:**闸1(C1 同 turn 内一次性)** —— prose-only 停止后,`no_tool_uses` 的 LHT 续跑 nudge 受 `long_horizon_continue_injected_this_turn` 限制**每 turn 只注入一次**,模型在同一 turn 内第二次以文字收尾即 `TurnComplete`。现改为**有质量工具进展即重置**该一次性闸(`observe_tool_result` 中 `qualifies` 时 `long_horizon_continue_injected_this_turn = false`),让续跑可在一个 turn 内随真实进展**多次触发**,上限仍由 `NudgeTracker` 的 `max_nudges_per_item` / `blocked_nudges_without_progress` 按进展兜底,不会空转。**闸2(C2 跨给-up 续跑)** —— 当 nudge 网关已 give-up(`blocked`/达上限/skip)但任务图仍**真实未完成**时,新增 `maybe_auto_continue_incomplete_lht` 兜底:在 `no_tool_uses` 最终 `Break` 前,若 `[long_horizon] auto_continue = true`,则清除 tracker give-up、重注入更强硬的「自动续跑」续跑消息(中英双语,明示**仅两种合法停止条件**:真实需用户决策的阻断 / 全部完成且验证通过),保持 turn 存活。每 turn 由新增 `[long_horizon] max_auto_continue_rounds`(默认 16)硬上限,真无法推进者仍会终止。新增遥测 `long_horizon.auto_continue` / `long_horizon.auto_continue_exhausted` 状态事件。`auto_continue` **默认关闭**,opt-in 用于无人值守多阶段跑。**Prompt(A):** `prompts/base.md` 新增「Run to completion(一推到底)」纪律段,讲清 handoff/cycle/清单清空/目标重注入**都不是停止信号**,只有上述两种情况才停。Files: `crates/core/src/long_horizon.rs`(配置 + toml 映射)、`crates/core/src/engine/{runtime,runtime_new}.rs`(per-turn 计数器)、`crates/runtime-server/src/core/engine/{message_handlers.rs(reset),turn_loop/host_impl/mod.rs(C1 重置),turn_loop/host_impl/no_tool_uses.rs(C2)}`、`crates/runtime-server/src/long_horizon/{nudge.rs(`build_auto_continue_message`+单测),mod.rs(导出)}`、`prompts/base.md`。
- **Fix (工具面审计 P2 第一批 — 体验/保真度快速优化):** 低风险 P2 一批:**① `exec_shell` `timeout_ms`** 工具层 `clamp(1000, 600_000)` 对齐 manager+schema。**② `apply_patch` fuzz** 默认 3(对齐 schema,上限 50)。**③ grep CRLF** 分行 strip `\r`。**④ git pathspec** Windows `\`→`/`(`git_pathspec_arg`)。**⑤ `list_dir`** 加 `offset`/`returned` 分页。**⑥ `edit_file`** 大文件跳过 diff;`delete_lines` clamp 注明。**⑦ `describe_image` webp。**⑧ `web_search`** 零结果 message 区分 challenge/解析失败。`tools::` 426/0。审计 v1.3。Files: `exec.rs`、`apply_patch.rs`、`search.rs`、`git.rs`、`git_history.rs`、`list_dir.rs`、`edit.rs`、`describe_image.rs`、`web_search.rs`。
- **Fix (工具面审计残余 P1 — cancel_token/grep IO/project_tree/web.run search):** 收尾后仍开放的 P1 项:**① C6 cancel:** `ssrf::read_body_capped`/`fetch_with_ssrf_guard` 绑定 `ToolContext.cancel_token`(chunk/redirect 可中断);`fetch_url`/`web_run/page`/`web_search` 透传。**② grep IO 误报:** 新增 `files_skipped_io`,与 `files_skipped_binary` 分离;`context_lines` clamp ≤20。**③ project_map 输出上限:** `project_tree_with_limit`(500 行)+ `tree_total_lines`/`tree_truncated`。**④ glob 边界:** `path` 不存在返回 `invalid_input`。**⑤ web.run search 对齐 web_search:** `check_host_policy`、DDG 失败/非 200/读失败 fallback Bing、流式读体 5MB。`tools::` 424/0。Files: `ssrf.rs`、`search.rs`、`project.rs`、`utils.rs`、`glob_files.rs`、`web_run/{search,tool}.rs`、`fetch_url.rs`、`web_search.rs`。审计 v1.2。
- **Fix (工具面审计收尾 — glob/file_search 语义 + C5 symlink 补全 + grep spawn_blocking/超时 + web_search 流式上限):** 审计 v1 实施后的最后一轮:**① backlog#13 glob 相对基准:** `glob_files` pattern 此前按 workspace 相对匹配,schema 写 relative to `path` → `path:"src"` + `*.ts` 漏匹配;改为 `strip_prefix(base_path)` 匹配、输出仍 workspace 相对路径;新增单测 `glob_files_pattern_relative_to_path_not_workspace`。**② `file_search` `respect_gitignore`:** 此前固定 true,无法搜被 ignore 文件;新增参数(默认 true,对齐 grep/glob)+ 返回字段;新增 `test_file_search_respect_gitignore_false`。**③ C5 补全:** `project.rs`/`utils.rs` 独立 walk(`summarize_project`/`project_tree`/`key_files`)仍 `follow_links(true)` → 改 false,与共享 `workspace_walk` 一致。**④ grep 阻塞/超时:** 文件 walk+scan 包进 `spawn_blocking`,外层 120s `tokio::time::timeout`,大 monorepo 不再占 async worker。**⑤ C6 web_search:** DDG/Bing HTML 改 `read_body_capped`(5MB 上限),保留读失败→Bing fallback 语义。`tools::` 423/0。Files: `glob_files.rs`、`file_search.rs`、`search.rs`、`web_search.rs`、`project.rs`、`utils.rs`;审计 [`TOOL_SURFACE_AUDIT.md`](docs/tech/TOOL_SURFACE_AUDIT.md) 升 v1.1。**仍待:** web `CancellationToken`、grep IO 误报 binary、`project_tree` 输出上限。
- **Fix (工具面审计 T7h-3 — git 子进程超时 / C6):** `git_*` 工具的 `run_git_command`(`git.rs`/`git_history.rs`)此前 `tokio` 异步但**无超时**,卡在凭据提示或锁等待会无限挂起。现加 30s `tokio::time::timeout` + `kill_on_drop(true)`,并设 `GIT_TERMINAL_PROMPT=0` 关闭交互式凭据提示(最常见挂起源)。注:`spawn()` 默认继承 stdio(不同于 `output()`),故显式 `Stdio::piped()` 以免 `wait_with_output()` 抓不到输出。`tools::git*` 9/0。Files: `crates/runtime-server/src/tools/{git,git_history}.rs`。
- **Fix (工具面审计 T7h-2 — `run_tests`/`office_write` 子进程超时 + kill / C6 孤儿防护):** `run_tests` 此前用 `cargo.output().await` **无超时**,hung 测试/构建会无限阻塞工具与其进程树;现加 `timeout_ms`(默认 600s,硬上限 1800s)+ `kill_on_drop(true)`,超时后 Windows 额外 `taskkill /T /F` 树杀 rustc/测试二进制孤儿。`office_write` 原有 120s `wait_timeout` 但超时分支**直接 drop child 而不 kill** → 留 Python 孤儿 + 输出文件锁残留;现超时即 `child.kill()`+`wait()`+Windows 树杀。新增 `run_tests_times_out_and_kills` 测试。`tools::` 421/0、`tools::test_runner` 4/0。Files: `crates/runtime-server/src/tools/{test_runner,office_write}.rs`。剩:git 子进程超时与 web `CancellationToken` 绑定待后续。
- **Fix (工具面审计 T7h-1 — web 抓取响应体流式上限 / C6 OOM 防护):** `fetch_url` 与 `web_run` 此前用 `resp.bytes().await` 把**整个**响应体读进内存,再事后 `[..max_bytes]` 截断 —— 多 GB / 无界响应会在截断前就 OOM。新增 `tools::ssrf::read_body_capped(resp, max)`,按 `chunk()` 流式累积,超过上限即停并返回 `(bytes, truncated)`,内存恒定在 ~上限。`fetch_url` 用其 `max_bytes`(≤10MB);`web_run` 用硬上限 `MAX_PAGE_BYTES`(25MB,此前**完全无上限**)。`tools::` 420/0。Files: `crates/runtime-server/src/tools/{ssrf,fetch_url}.rs`、`crates/runtime-server/src/tools/web_run/page.rs`。注:`CancellationToken` 绑定(取消后请求仍跑满 timeout)仍待后续。
- **Fix (工具面审计 T7f-2 — `apply_patch` 编码保留 / C8 收尾):** `apply_patch` 读侧此前用 `read_to_string`(非 UTF-8 报错)、写回/回滚用裸 `content.as_bytes()`(GB18030 文件会被转成 UTF-8)。改为读侧 `read_decoded_for_edit`,`PendingWrite` 新增 `encoding`/`had_bom` 字段透传原编码(新文件默认 utf-8),`apply_pending_writes` 与 `rollback_pending_writes` 均经 `encode_text` 按原编码写回。删除现已无用的 `read_file_content`。新增 GB18030 往返测试。`tools::apply_patch` 18/0。File: `crates/runtime-server/src/tools/apply_patch.rs`。至此 C8(write/edit/fim/patch)全部缓解。
- **Fix (工具面审计 T7f — `write_file`/`edit_file`/`fim` 编码保留 / C8):** 此前 `write_file` 覆盖 GB18030 文件会静默转成 UTF-8,`edit_file`/`fim` 用 `read_to_string` 遇非 UTF-8 直接报错(GB18030 文件无法改)。新增 `tools::file::encode_text`(支持 `utf-8`/`utf-16le`/`utf-16be`(含 BOM 复原)/`gb18030`/`windows-1252`,未知编码安全退回 UTF-8;encoding_rs 无 UTF-16 编码器,手写)与 `read_decoded_for_edit`(读+容错解码,返回 `{text,label,had_bom}`)。`write_file` 捕获原编码并按其回写;`edit_file` 四个 operation 与 `fim` 改走解码读 + 原编码原子写。新增 6 个测试(GB18030/UTF-16LE/BE 往返 + write/edit 保留)。`tools::file` 46/0。Files: `crates/runtime-server/src/tools/file/{write,edit,mod}.rs`、`crates/runtime-server/src/tools/fim.rs`。`apply_patch` 编码保留待后续(仍 UTF-8)。
- **Fix (工具面审计 T7g — `shell_output` summary 从尾部扫高信号行 / C7):** `summarize_output` 此前取输出**头** 3 行,cargo/测试输出会被 `Compiling`/`running N tests` 占满,真正的结论 `test result:`(在尾部)被丢,模型据此误判通过/失败。改为对全文(含截断时追加的 “Preserved summary lines” 块)**反向扫描** `is_summary_line`,取最后 ≤3 条高信号行(`test result:`/`failures:`/`error[`/`panicked`/退出码),仅在无任何高信号行时才退回头部 3 行。新增 3 个测试(尾部结论/无信号退回头部/截断保留块)。`tools::shell_output` 7/0、`tools::shell` 25/0。File: `crates/runtime-server/src/tools/shell_output.rs`。
- **Fix (工具面审计 T7d — `file_search` 报总数/截断标志 / C7 静默截断):** `file_search` 此前返回裸 match 数组,超 `limit` 直接 `truncate` 且不报总数,模型会误以为命中只有 N 个。改为返回对象 `{matches, total_matches, returned, truncated}`(对齐 `grep_files` 的 `total_matches`/`truncated` 形态);`total_matches` 为截断前的全量评分命中数,`truncated` 在 `total_matches > returned` 时为 `true`。新增 2 个测试覆盖截断/未截断两路。`tools::file_search` 5/0。File: `crates/runtime-server/src/tools/file_search.rs`。
- **Fix (工具面审计 T7e — `edit_file`/`fim` 原子写 / P1 防截断文件):** `edit_file`(4 处 operation 出口)与 `fim` 用 `fs::write` 直接覆盖目标文件,崩溃/磁盘满会留下半截内容(`write_file`/`apply_patch` 早已用 `atomic_write`)。改为复用 `tools::file::write::atomic_write`(同目录写 temp + `fs::rename` 原子替换,Windows 亦支持覆盖 rename)。`tools::file` 34/0。Files: `crates/runtime-server/src/tools/file/edit.rs`、`crates/runtime-server/src/tools/fim.rs`。
- **Fix (工具面审计 T7a-c — 搜索/walk 跨平台与编码健壮性 / C5 symlink + grep UTF-16 + grep Windows glob):** 三处搜索类缺陷:**① C5 symlink 越界(安全 P1):** 共享 `workspace_walk` 用 `follow_links(true)`,工作区内一个指向区外的符号链接会让 grep/glob/file_search/project_tree 读到工作区外文件(walk 出的路径不再过 `resolve_path`)。改 `follow_links(false)`——**亦与 ripgrep 自身默认一致**。**② grep UTF-16 搜不到(P0):** `is_probably_binary` 见前 8KiB 有 NUL 即判二进制、**先于** `detect_and_decode`,而 UTF-16 文本(ASCII 内容每隔一字节即 NUL)因此被当二进制跳过(GB18030 有单测、UTF-16 此前实际搜不到)。改为对 UTF-16 LE/BE BOM 与 UTF-8 BOM **放行**,NUL 启发式仅作用于无 BOM 文件。**③ grep Windows glob(P1):** `grep_files` 的 include/exclude `matches_glob` 按 `/` 切分,但 Windows 相对路径用 `\` → `src/**/*.rs` 漏/多扫。匹配前把相对路径 `\`→`/`(`glob_files` 已有此法)。`tools::search` 18/0、`glob_files` 2/0、`file_search` 3/0、`project` 全过。Files: `crates/runtime-adapters/src/tools/workspace_walk.rs`、`crates/runtime-server/src/tools/search.rs`。
- **Fix (工具面审计 T6 — async 内同步 `Command::output()` / blocking reqwest 阻塞 tokio worker / C4 P1):** 多个 async 工具在异步上下文里同步 `Command::output()`(`git`/`git_history`/`test_runner`/`diagnostics`)或 `reqwest::blocking`(`describe_image`)——会占住一个 tokio worker 线程直到子进程/HTTP 完成,长跑里并发工具相互拖累、甚至饿死调度。修复:`git.rs`/`git_history.rs`/`test_runner.rs` 的 `run_git_command`/`run_cargo` 改 `tokio::process::Command` + `.output().await`(调用处加 `.await`);`diagnostics.rs` 的探测树(`probe_git` + 两个 `probe_version`,深层 sync 助手不便逐个改)整体包进 `tokio::task::spawn_blocking`;`describe_image.rs` 的 `VisionClient::call` 改异步 `reqwest::Client` + `.send().await`/`.json().await`。顺带移除因此空出的 `use std::process::Command` 导入。`tools::git` 9/0、`git_history` 含其中、`diagnostics` 2/0、`test_runner` 3/0、`cargo check` 全绿。**残留(归 C6/T7):** git/test_runner 子进程仍无超时。Files: `crates/runtime-server/src/tools/{git.rs,git_history.rs,test_runner.rs,diagnostics.rs,describe_image.rs}`。
- **Fix (工具面审计 T5 — SSRF 重定向/分支复校验 IP / C3 P0 安全):** 两处出网 SSRF 缺口:① `fetch_url` 只对**初始 URL** 校验内网 IP,`redirect(Policy::limited(5))` 跟随 302 后**不再复校验**——公网→302→`169.254.169.254`(云元数据)可被跟到;② `fetch_url` DNS 解析失败时**跳过**检查放行(与成功路径策略不一致);③ `web_run/page` 取页**完全无 IP 阻断**,仅查 network policy。修复:新增共享模块 `crates/runtime-server/src/tools/ssrf.rs`——`fetch_with_ssrf_guard` 用 `Policy::none()` **手动跟随重定向**,**每一跳**目标 host 都过 `check_url_policy` + localhost 阻断 + `is_restricted_ip`(私网/loopback/link-local/元数据),并 pin 校验后 IP 关闭 DNS-rebinding TOCTOU 窗口;DNS 失败/零地址**fail closed**(拒绝而非放行);非 http(s) 重定向目标拦截;超 5 跳报错。`fetch_url` 与 `web_run/page` 改为共用此 guard(消除 `web_run` 此前零 IP 阻断)。新增单测 5 条(metadata IP / 私网+loopback+`::1` / localhost / DNS 失败 fail-closed / 公网 IP 放行);`tools::ssrf` 5/0、`fetch_url` 7/0、`web_run` 11/0。Files: `crates/runtime-server/src/tools/ssrf.rs`(新增)、`crates/runtime-server/src/tools/mod.rs`、`crates/runtime-server/src/tools/fetch_url.rs`、`crates/runtime-server/src/tools/web_run/page.rs`。
- **Fix (工具面审计 T4 — sync 路径 reader 无界 join / P0 防挂死):** `ShellManager::execute(..,false)` 的同步路径 spawn 两个 `read_to_end` 线程收集 stdout/stderr,收尾时**无界 `join()`** —— 与 CCR 那次背景路径挂死同源:存活的 grandchild 持着管道写端时 `read_to_end` 永不 EOF,`join()` 永久阻塞。背景路径早已用 `join_reader_bounded` 缓解,sync 路径未对齐。修复:新增 `join_reader_thread_bounded`(返回 `Vec<u8>` 的有界 join,与既有 `join_reader_bounded` 同 `READER_DRAIN_GRACE`=500ms detach 策略,只是 sync 线程返回 buffer 而非写共享 buf);把 sync 路径**成功出口与超时出口**两处的 stdout/stderr join 全部替换。新增两单测(detach 返回空 buffer / 正常返回 buffer);`tools::shell` lib 29/0。Files: `crates/runtime-server/src/tools/shell/process.rs`、`crates/runtime-server/src/tools/shell/manager.rs`。
- **Fix (工具面审计 T3 — Windows 杀进程树 / C1 P0 根治孤儿占端口):** Windows 无进程组 SIGKILL,`child.kill()`(→`TerminateProcess`)只终止直接子进程,被测命令经 `Start-Process`/守护进程化派生的孙进程变孤儿、继续占监听端口(长程跑里反复见到的 7878/6379 泄漏即此)。修复:新增 `kill_process_tree(pid)` 走 `taskkill /T /F /PID <pid>`(walk 整树强杀,错误吞掉、调用方再 reap 直接子进程);非 unix `kill_child_process_group` 改为先树杀再 `child.kill()`;`ShellChild::kill` 两平台统一走 `kill_child_process_group`,使 **Drop / `BackgroundShell::kill` / cancel 工具 / manager 两处 sync 超时 kill** 全部自动受益(顺带消除 manager 处 `kill_child_process_group` 在 unix 下未 import 的潜在编译缺口)。新增 `#[cfg(windows)]` 单测 `test_exec_shell_kill_terminates_grandchild_process_tree`(父进程记录其 Start-Process 孙进程 PID → kill 父 → 断言孙进程已终止);`tools::shell` lib 27/0。**后续:** Job Object(`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`)更彻底但需改多条 spawn 路,taskkill 为轻量足够方案。Files: `crates/runtime-server/src/tools/shell/process.rs`、`crates/runtime-server/src/tools/shell/manager.rs`、`crates/runtime-server/src/tools/shell/tests.rs`。
- **Fix (工具面审计 T2 — `edit_file` 空 `search` 防呆 / P0 防丢数据):** `edit_file` 的 `search_replace` 此前不拒绝空 `search`,而空串(或纯空白)在 `replace_mode:"all"` 下会在每个 UTF-8 边界命中 → 把 replace 文本插到每个字节之间 → **破坏整文件**。修复:取到 `search` 后立即校验 `search.trim().is_empty()`,空则返回 `invalid_input` 并导向 `insert_after`/`replace_line`(`edit.rs:149-158`)。新增单测 `test_edit_file_empty_search_rejected`(空串/空格/`\n\t ` 三态均报错且文件字节不变)。Files: `crates/runtime-server/src/tools/file/edit.rs`、`crates/runtime-server/src/tools/file/tests.rs`。
- **Fix (工具面审计 T1 — foreground `exec_shell` 透传 `cwd` / C2 已核实快速修):** 默认前台 `exec_shell` 走 `execute_foreground_via_background` 包装时把 `working_dir` 硬传 `None`(`helpers.rs:66-68`),回退到工作区根 —— background/interactive 分支(`exec.rs:296/308`)却都正常传 `working_dir.as_deref()`。这比"shell 无状态"更深一层地解释了 MicroStack 那类 `go mod init`/`go build` 落错目录:模型即便正确传了 `cwd` 也被静默丢弃。修复:给 `execute_foreground_via_background` 加 `working_dir: Option<&str>` 形参并在内部 spawn 时透传,`exec.rs:318` 调用处传同作用域已解析好的 `working_dir.as_deref()`(经 `resolve_path` 过 workspace 边界)。新增单测 `test_exec_shell_foreground_respects_cwd`(`cwd:"nested"` 下相对写文件落在子目录、不泄漏到工作区根)通过。**残留:** OpenSandbox `backend.exec`(`exec.rs:210`)仍不带 cwd —— 涉及 `SandboxBackend` trait + 远程 sandbox 协议(`SandboxRunRequest`)改动,另议。Files: `crates/runtime-server/src/tools/shell/tools/{helpers.rs,exec.rs}`、`crates/runtime-server/src/tools/shell/tests.rs`。
- **Fix (MicroStack03 长 session 复盘吸收 — LoopGuard 抗迭代误拦 + 门禁工程根自适应 + 工具易用性):** 一次 ~4500 行 Go 框架长程跑(产物 `F:\LHT_TEST\MicroStack03`)复盘出 Zagens 几处真实摩擦,逐条核实后修复(非全采纳模型自述——其中"cwd 竞态"实为 shell 无状态、"子代理无硬约束"实已有 `[features] subagents=false` 硬开关):**①【最高价值】LoopGuard 迭代误拦** —— `IDENTICAL_CALL_BLOCK_THRESHOLD=3` 按 `(tool, args-hash)` 整 turn 累计且永不衰减,导致"改代码→重跑同一条 `go test`"在第 3 次被拦,模型被迫靠调换 flag 顺序改 hash 绕过(反而架空防循环)。新增 `LoopGuard::note_state_changed()`(清 `call_counts`)+ `is_state_mutating_tool()`,在 `tool_phase.rs` 成功执行 `write_file`/`edit_file`/`apply_patch`/`create_dirs` 后调用 —— 工作区变了则同条 verify 不再算空转、放行;**无改动仍连发同条仍照拦**(防循环初衷不变)。**② 门禁工程根自适应** —— 模型把整个 Go 工程嵌进 `microstack/` 子目录(`go.mod` 不在工作区根),`auto_verify_replay` 复跑 `go build ./contracts` 与 `toolchain_gate` 探测都以工作区根为 cwd → 误判 "no go.mod"。新增 `resolve_project_root(workspace)`:根无构建标记但**恰有一个**子目录有(`go.mod`/`Cargo.toml`/`package.json`/`pyproject.toml`/`pom.xml`/`build.gradle*`)则下探一层(深度 1,歧义则守在根),`detect_toolchain_entries` 与 verify 执行共用该根;层3 交付物对账仍用工作区根(算子 manifest 语义不变)。**③ edit_file 参数名提示** —— 误用 `new_str`/`new_string`/`replacement` 时给定向提示"用 `replace`",schema 描述同步点明;**④ prompt 加固**(`base.md`):exec_shell **跨调用无状态**(`cd` 不持久,用 `cwd` 参数或单条 `&&`)、`write_file`/`apply_patch` **自动建父目录**(勿用非可移植的 `mkdir -p`)、gofmt 后 **重读再 edit**、**边做边勾 checklist**(进度由清单驱动、非文件数);**⑤** `config.example.toml` 写清 `max_subagents` 0/负被钳为 1、硬禁子代理用 `[features] subagents=false`。新增/通过单测:LoopGuard 状态变更放行 + 仅文件类工具触发(core 13/0)、`resolve_project_root` 三态(根/单层下探/歧义)、edit_file 别名提示 ×2;`tools::file` 36/0、`long_horizon` 48/0、`cargo check` 全绿。Files: `crates/core/src/engine/{loop_guard.rs,turn_loop/tool_phase.rs}`、`crates/runtime-server/src/long_horizon/{generic_gate.rs,completion_gate_flow.rs}`、`crates/runtime-server/src/tools/file/{edit.rs,tests.rs}`、`crates/runtime-server/src/prompts/base.md`、`config.example.toml`。
- **Docs (工具面审计 v1 — MicroStack03 压测复盘延伸 / 工具健壮性·跨平台·边界·效率四维实证清单):** 压测暴露的最大弱点是**工具功能本身**而非模型能力,故对 `crates/runtime-server/src/tools/**`(99 文件)做一次只读实证审计,新增 [`docs/tech/TOOL_SURFACE_AUDIT.md`](docs/tech/TOOL_SURFACE_AUDIT.md):按 shell/file/search/web 四类逐工具列出带 `文件:行号` 的缺陷 + 严重度(P0 挂死/丢数据/安全 · P1 长任务高频痛点 · P2 体验),并抽出 8 条跨工具共性主题。**已二次人工核实的关键发现:** 默认 **foreground `exec_shell` 静默丢弃 `cwd` 参数** —— `exec.rs:318` 走 `execute_foreground_via_background` 时硬传 `working_dir=None`(`helpers.rs:66-68`),而 background/interactive 分支(`exec.rs:296/308`)都正常传 `working_dir.as_deref()`;这比"shell 无状态"更深一层地解释了 MicroStack 的 `go mod init` 落错目录(模型即便传了 `cwd` 也无效),列为 P1 快速修。**其余高价值条目(未修,登记 backlog):** P0——Windows 杀不掉进程树(`child.kill()` 只杀直接子进程→孤儿占端口,即今日 7878)、`fetch_url`/`web_run` 重定向不复校验 IP 的 SSRF、`edit_file` 空 `search`+`replace_mode:all` 破坏整文件、`grep_files` UTF-16 被 NUL 嗅探挡在解码前、sync 路径 reader 无界 join;P1——多处 async 内同步 `Command::output()` 阻塞 worker、`workspace_walk` `follow_links` 跟符号链接出工作区、git/test/office 子进程无超时、`write_file` 把 GB18030 静默转 UTF-8 而 `edit_file`/`apply_patch`/`fim` 仅 UTF-8、`edit_file`/`fim` 非原子写、`file_search` 截断不报 total。审计为 backlog 快照(非已修清单),修复时回填状态。Files: `docs/tech/TOOL_SURFACE_AUDIT.md`。
- **Docs (组合式 Harness 回归夹具去"假门" — `microstack-completion-gate.toml` 退出码恒 0 的门改真门):** 修复该回归夹具自身两道**形同虚设的门**（[`fixtures/microstack-completion-gate.toml`](docs/harness/fixtures/microstack-completion-gate.toml)，对齐 §11 H2 跨平台覆盖率指引）：**① gofmt** —— `gofmt -d`/`-l` 退出码恒 0、只打印输出，糊弄退出码 oracle；改 `shell="bash"` 用 `test -z "$(gofmt -l .)"` 把"有未格式化文件"转成 exit 1。**② 覆盖率** —— 裸 `go test ./... -cover` 退出码恒 0（覆盖率 4% 也绿），改 `shell="bash"` 真门 `coverage_per_package_min75`：先 `&&` 保证全部测试通过、再对**每个有测试的包**卡 ≥75%（§1-B 防稀释，禁止 contracts 100% 把 app/orm 低覆盖率稀释成总体达标），并注明跨平台正解仍是仍未实现的内置 `coverage-gate` 子命令（`shell=none`+argv）、bash 门仅作 *nix 退路。顺带：Router trie 交付物 glob 由假设顶层 `router/` 放宽为 `**/*trie*.go`（结构交模型自定）；补**前置条件**头注（产物目录须为 git 仓库且 contracts 已 commit，否则 `git diff --exit-code` 与 `tracked=true` 无基线恒红）+ bash/unix 工具依赖说明 + §6.5 零 per-task 全局开关（`auto_verify_replay`/`toolchain_gate`）示例。Python `tomllib` 解析校验通过（6 verify + 5 deliverable）。Files: `docs/harness/fixtures/microstack-completion-gate.toml`。
- **Feature (组合式 Harness — 任务无关的层2完成门禁 / 零 per-task 配置覆盖所有代码任务):** 针对"手写 per-task manifest 不可规模化"的根本局限（[`COMPOSABLE_HARNESS.md` §6.5/v0.6](docs/harness/COMPOSABLE_HARNESS.md)），把层2 的价值内核"harness 主动跑、退出码当法官"从算子 manifest 解耦，新增**两个零 per-task 配置、各一个全局开关即覆盖所有代码任务**的来源：**① 模型 `[verify:]` 复跑**（`auto_verify_replay`）——收尾时主动 exec 已完成 checklist 项里模型**自己声明**的 `[verify: cmd]`，把"声称跑过"升级为退出码 oracle；**无新增授信面**，命令本就在模型既有 exec 权限内；**② 工具链探测门**（`toolchain_gate`）——探测 workspace 根 `go.mod`/`Cargo.toml`/`package.json`/`pyproject.toml`/`pom.xml`/`build.gradle*` 跑 canonical build/test。三来源（operator/model/toolchain）按归一化命令**合并去重**（优先级 operator>toolchain>model）一次跑完，**每条按自身来源的 mode（`mode`/`auto_verify_replay`/`toolchain_gate` ∈ off|observe|enforce）裁决**，单轮可混合 enforce（强制返工）与 observe（仅记录）；infra-strike 只在 **enforced 失败子集**累计（observe 环境错误绝不逼模型）；层3 仅在无 enforced 层2 失败时进入（§7.7 同轮 trust）。全 off 且无算子 manifest → 行为与现状逐字节一致。**遥测/面板：** `manifest_gate_result` 载荷新增 `sources{operator,model_declared,toolchain}` + `enforced_failing`/`observed_failing`；LHT 面板摘要卡新增"通用层2：[verify:] 复跑 X · 工具链门 Y"行（四语言 i18n）。算子 manifest 据此**重新定位**为回归夹具 + 少数高价值任务的层3交付物对账。新增单测（`[verify:]` 提取/去重、工具链探测、合并优先级、来源降级）；`long_horizon` 45/0、core completion_gate 2/0、web-ui 严格 TS 构建全绿。Files: `crates/core/src/long_horizon/completion_gate.rs`、`crates/runtime-server/src/long_horizon/{generic_gate,completion_gate_flow,gate_telemetry,completion_audit,manifest_gate,nudge,completion_gate_panel}.rs`、`.../runtime_threads/manager.rs`、`crates/desktop/web-ui/src/{components/LongHorizonPanel.tsx,lib/types/longHorizon.ts,i18n/locales/*}`。
- **Fix (组合式 Harness 实施审查吸收 — 安全链路补齐 + 来源护栏 + observe 语义):** 对 P0/P1/P2 落地代码逐文件审查后补齐四项（[`COMPOSABLE_HARNESS.md` v0.5](docs/harness/COMPOSABLE_HARNESS.md)）：**①【安全链路】** 层2 `shell=none` 的 argv 直执路径此前**绕过** `analyze_command` 危险命令分析（仅 shell 包装路径有），现两条路径都过危险分析；据实校正 §6.4——shell 路径走 `ShellManager.execute_with_options_env` 已继承该 manager 的 sandbox policy、argv 路径因 manifest 仅来自受信来源故直执不另套 sandbox。**②【enforce 来源护栏】** 新增 `CompletionGateConfig::sanitized_for_source(trusted)`：非受信来源的 `enforce` 自动降级 `observe`，杜绝未受信 manifest 自动执行命令；当前 loader（`resolve_load_config_path`）只读单一受信路径、不合并 workspace 项目配置，故 `long_horizon_config()` 传 `true`，并留注释要求未来 workspace overlay 必须传 `false`。**③【observe 语义】** `infra-strike` 的 `audit_unmet(gate_infra_error)` 改为**仅 enforce 触发**，observe 维护计数器但绝不强停（§7.3，保留「模型能否自驱完成」的观测价值）。**④【shell 引号加固】** `wrap_shell_command` 改用单引号转义（pwsh `''`、bash `'\''`）、cmd 双引号包裹，复杂命令仍推荐 `shell=none`+argv。新增/通过单测：argv 危险分析、`classify_command_not_found_as_infra`、各 shell wrap、来源降级（core ×2 + runtime ×4）；`long_horizon` 测试 39/0、`cargo check` 全绿。Files: `crates/runtime-server/src/long_horizon/{manifest_gate,completion_gate_flow}.rs`、`crates/core/src/long_horizon/completion_gate.rs`、`crates/runtime-server/src/config/load/impl_config.rs`。
- **Feature (组合式 Harness P2 — LHT 面板 + 离线 grep):** LHT **Nodes** Tab 顶栏展示 `completion_gate` 摘要（mode、层2/层3 轮次、`first_gap_count`、`gate_reinject_while_blocked`、最近 manifest/audit 结果、`audit_unmet` 原因）；节点流着色 `manifest_gate_start`/`manifest_gate_result`/`completion_audit`/`audit_unmet`；`harness/task-graph` 载荷新增 `completion_gate`（`completion_gate_panel.rs` + `manager.rs` 遥测缓存）。文档：[`COMPOSABLE_HARNESS.md` §11](docs/harness/COMPOSABLE_HARNESS.md)、[`microstack-framework.md` §5–§7](docs/harness/test-cases/microstack-framework.md)。Files: `crates/desktop/web-ui/src/components/LongHorizonPanel.tsx`、`.../lib/types/longHorizon.ts`、`.../i18n/locales/*`、`crates/runtime-server/src/long_horizon/completion_gate_panel.rs`、`.../runtime_threads/manager.rs`。
- **Feature (组合式 Harness P0+P1 — manifest 完成门禁 + 缺口补齐):** 按 [`docs/harness/COMPOSABLE_HARNESS.md`](docs/harness/COMPOSABLE_HARNESS.md) 在 `graph_complete` 前插入组合门禁（无 manifest 时行为与现状一致）。**层2:** harness 经 `ShellManager` 主动执行 verify manifest；**层3:** `completion_audit.rs` 工作树 path/glob/`tracked` 对账 + **`optional_verify_cmd` 真跑**。**遥测:** `manifest_gate_start` / `manifest_gate_result`（含 JSON detail）/ `completion_audit`（§6.3 结构化 JSON）经 `[lht-probe]` tee。**护栏补齐:** `max_infra_strikes` 连续 infra → `audit_unmet(gate_infra_error)`；`steps_remaining==0` 且门未绿 → `steps_and_manifest_exhausted`；`suppress_git_progress_baseline` 排除 gate 副作用污染 `progress_via_git`。**文档/夹具:** [`docs/harness/fixtures/microstack-completion-gate.toml`](docs/harness/fixtures/microstack-completion-gate.toml)、[`microstack-framework.md` §7](docs/harness/test-cases/microstack-framework.md)。Files: `crates/core/src/long_horizon/`、`crates/runtime-server/src/long_horizon/{manifest_gate,completion_audit,completion_gate_flow,gate_telemetry,progress}.rs`、`.../no_tool_uses.rs`。
- **Fix (上下文面板长 turn 内冻结到回合末才更新 — op-loop mid-turn 饿死 + 回退陈旧 store 快照 / Q1 实证):** 用户观察到「长程任务 → 上下文」面板在任务进行中不动、只在回合结束才刷新。逐行核实根因:`panel.context` 的活值由 `emit_panel_context → get_thread_context` 产出,后者**先试活引擎 op `query_context_snapshot()`(5s 超时)**,但长 turn 内引擎 op loop **被饿死**(turn loop 只 drain steer/cancel,与历史 task-graph 同款,见本节下方旧条目),于是超时 → **回退从 DB 重建消息**算快照,而 turn 进行中新消息未必落库 → 读到的是**上一回合的陈旧值**,直到回合末 op loop 恢复才更新。这对多小时压力跑尤其致命:整个长 turn 里压力条全程陈旧、看不到 cycle 逼近。修复(仿 `checklist_persist` 对账通道,绕开饿死 op):引擎在**每个 per-step 安全边界**(`maybe_advance_cycle_at_checkpoint`,LHT 代码任务、stream+tool 已结束的干净点)用 `engine_context_snapshot()` 算出 live 快照,经 harness-status 通道发 `long_horizon.context_snapshot:<json>`;monitor 识别该前缀(与 checklist_persist 并列,**不建 timeline 项**、且因每步触发**不进 `[lht-probe]` tee 避免刷屏**)转 `observe_harness_status`;host 解析后调新增的 `emit_panel_context_snapshot` **直接用引擎预算好的快照推 `panel.context` SSE**(不再 re-query、不触饿死 op)。前端 `panel.context` 消费端不变。成本可忽略——cycle 门本就每步算 `estimated_input_tokens`。Files: `runtime-server/src/core/engine/turn_loop/host_impl/mod.rs`、`runtime-orchestrator/src/runtime_threads/monitor.rs`、`runtime-server/src/runtime_threads/monitor_host.rs`、`runtime-server/src/runtime_threads/manager.rs`。
- **Fix (`topic-memory` markdown 清洗正则含未闭合嵌套字符类 — clippy `invalid_regex` 报错 + 运行时形同失效):** `crates/topic-memory/src/extract.rs` 的 `Regex::new(r"[#*_~>|[\]()]+")` 字符类里有未转义的内层 `[`,clippy 在编译期 `invalid_regex` 直接报错(挡住依赖该 crate 的 `cargo clippy`),且运行时 `Regex::new` 返回 Err → 该 markdown 标点清洗步骤静默 no-op。改为 `r"[#*_~>|\[\]()]+"`(转义 `[`)。`cargo clippy -p zagens-topic-memory` 由报错转为仅余 6 条既有 style warning。Files: `crates/topic-memory/src/extract.rs`。
- **Fix (cycle 阈值彻底不可配 — `[context] cycle_threshold` / `[cycle.per_model]` 是死配置、checkpoint-restart cycle 永远卡在写死的 768K):** 排查"LHT 命门 cycle 到阈值能否准确触发"时逐行核实出一处真实缺口:驱动真 cycle 的 `should_advance_cycle` 读的是引擎 `self.config.cycle`,而它在 `runtime_threads/engine_spawn.rs` **恒为 `CycleConfig::default()`**(v4-pro 写死 768K),从不读任何配置;用户文档化的 `[context] cycle_threshold` 只流向 `SeamManager`(`core/engine/build.rs`,且 `[context] enabled` 默认关),core 注释声称从 `[cycle.per_model]` 加载的 `ModelCycleConfig` 在 runtime-server **从未被反序列化**——三处配置全是死的。后果:1M 窗上 cycle 触发线(`trigger_floor = min(768K, window−headroom≈785K) = 768K`)虽**低于**溢出裁剪线(785K)、逻辑自洽(工具把活跃输入顶过 768K 后步末干净 cycle 门 `run.rs:343` 会先于溢出触发),但 768K 在 1M 窗上**实际几乎不可达、更无法在合理时间内验证**,且操作者**无任何手段**为不同窗口/工作负载调阈值或做一次低阈值验证跑。修复:新增 `Config::cycle_runtime_config(model)`(仿 `compaction_runtime_config`),从 `CycleConfig::default()` 出发应用 `[context] cycle_threshold`(全局)与 `[context.per_model.<model>] cycle_threshold`(按模型)覆写,在 `engine_spawn` 用它替换写死的 default;**默认无覆写时仍是 768K,压力测试语义零变化**。要点:`CycleConfig::threshold_for` 先查 `per_model`、而 default 给 V4 模型种了 768K 种子项,故全局覆写必须**同时重写那些种子**否则对 V4 被遮蔽(已加单测固化)。同时把 harness `cycles` 面板载荷里写死的 `DEFAULT_CYCLE_THRESHOLD_TOKENS` 改为透传**实际配置阈值**(`build_cycles_value` 新增入参,两调用点分别传引擎 `config.cycle.threshold_for(model)` 与兜底 `cycle_runtime_config`),使「Cycle 阈值约 N / 换脑约 X%」在低阈值验证跑时显示真实值而非永远 768K/77%。验证:3 个新单测(默认仍 768K、全局覆写正确重写 V4 种子、per-model 覆写胜出)全过;`cargo check -p zagens-cli` 通过。Files: `runtime-server/src/config/load/impl_config.rs`(+`config/load/tests.inc.rs` ×3)、`runtime-server/src/runtime_threads/engine_spawn.rs`、`runtime-server/src/long_horizon/cycles.rs`、`runtime-server/src/core/engine/platform_dispatch.rs`、`runtime-server/src/runtime_threads/manager.rs`。**顺带发现(未修,另列):** `crates/topic-memory/src/extract.rs:89` 的 `Regex::new(r"[#*_~>|[\]()]+")` 含嵌套未闭合字符类,clippy `invalid_regex` 报错、运行时 `Regex::new` 返回 Err 使该 markdown 清洗形同失效。
- **Docs (LHT 测试用例 — MicroStack Go 微服务框架案例新增):** 新增 `docs/harness/test-cases/microstack-framework.md`（案例编号 MICROSTACK）——目标 1.5–4 万行纯 Go 的最大规模 LHT 载体。相对解释器 / CCR 两类既有案例，**新增两条它们都覆盖不到的维度**：① **接口稳定性**（第一阶段冻结的 `contracts/` 接口零改动，`git diff --exit-code contracts/` 做客观锚点）；② **重构抗性**（Router 内部实现换 trie、外部 API + 全部 `go test` 仍全绿）。基于本轮讨论按第一性原理收口，落了三条非谄媚结论:(1) **体量大 ≠ 会触发 cycle**——默认 768K 阈值下与 CCR 同样未必触发，cycle 测试改走「验证跑 A」（低阈值 + 单长 turn + **不手动回溯接口**，避免人替 harness 做交接糊掉 `carry_forward` 信号）；(2) **重集成模块（Kafka/RabbitMQ/gRPC 服务发现/Redis 哨兵）为假绿高发区**——`go build + 自写单测覆盖率 80%` 可被编译桩 + mock 糊弄、从不碰真实中间件，默认不纳入验收，要测须起真实服务；(3) cycle 首测仍以 `redis-cycle-handoff.md`（REDIS-CYCLE，探针更尖）为准，MicroStack 定位为接口稳定性 + 重构抗性 + 大规模长程压测。交接保真探针对应 REDIS-CYCLE 的 `op_seq/WCOUNT`：**接口稳定性（git diff contracts/）+ X-Request-ID 端到端贯穿**两条横切不变量。Files: `docs/harness/test-cases/microstack-framework.md`。
- **Docs (组合式 Harness 设计草案 v0.3 — 第二轮代码核对修订):** 修订 `docs/harness/COMPOSABLE_HARNESS.md`（v0.2→v0.3）。逐行核实方案对代码的全部论断属实（`maybe_continue_incomplete_code_task` 根因、DEMO3 guard 只遍历已完成 checklist 项、`verify.rs`/`nudge.rs` 可复用件、`record_long_horizon_tool_outcome`→`recent_verification_cmds`、`EngineRuntimeExt.long_horizon_state`、scratchpad/subagent 现状）后，吸收 7 条审核意见：**C1（核心）** 解决「层3 LLM 在 oracle 铁律下无非自由裁量职责」的自相矛盾——层3 **去 LLM 化**为纯 Rust 对账模块 `completion_audit.rs`，删除 headless LLM runner / `agent_spawn` 依赖，P1 不再是 LLM 阻塞项；**C2** §7.9 诚实声明适用边界（完成度上界 = manifest 完整度，非规格散文，「模型欠拆」平移为「算子欠写 manifest」）；**H1/H2** 新增 §6.4「层2 主动执行模型」（Windows/shell 选择、cwd、超时、crash vs assertion 的 `exit_class`、副作用边界、sandbox 授信）+ §6.1 覆盖率门改仓库内置跨平台子命令（不外包手写 bash）；**H3** §7.8 定 gate 返工优先于 `NudgeTracker.Blocked`/`max_nudges`；**H4** §7.7 定层2→层3「同轮」原子边界（中间不得插模型 step、cache 不跨轮）；**M1** §10 P0 独立受益场景写清；**M2** §10 observe 模式 P0 最小遥测落点；**S1** §5.1 修正 `long_horizon_state` 字段路径。Files: `docs/harness/COMPOSABLE_HARNESS.md`。
- **Docs (组合式 Harness 设计草案 v0.4 — 安全与执行契约收紧):** 修订 `docs/harness/COMPOSABLE_HARNESS.md`，把实现前风险固化为硬约束：enforce manifest 可执行命令只接受用户全局配置 / 内置测试夹具 / 明确受信算子配置，workspace/issue/模型生成内容默认只可 observe；`shell=none` 强制 `argv`，禁止 runtime 拆分字符串；层3 `path`/`glob` 默认对账 workspace 工作树，只有 `tracked = true` 才要求 `git ls-files`，避免已生成但未暂存文件误判缺失；层2执行必须走与 `exec_shell` 等价或更强的安全链路（exec policy、危险命令分析、sandbox、取消/超时、审计），并补 canonicalize/no-escape、gate start/result 事件、防并发重入；层2副作用不得污染 `progress_via_git`。Files: `docs/harness/COMPOSABLE_HARNESS.md`。
- **Docs (组合式 Harness 设计草案 v0.2 — Plan 审核修订):** 修订 `docs/harness/COMPOSABLE_HARNESS.md`（v0.1→v0.2）。吸收代码对照审核：层2 明确为 harness 主动 manifest exec（不依赖 `recent_verification_cmds`）；层3 交付物改为算子显式 manifest + runtime headless audit runner（非模型 `agent_spawn`、非 `scratchpad/auditor.rs`）；收窄可复用件表；新增 observe/enforce 模式、独立轮次计数、step 预算交互；核实模型侧 exit 0 记录已有并写清 P0/P1 能力边界（欠拆解负样本须 P0+P1）；补 manifest TOML/JSON schema 草案。`microstack-framework.md` §5 增交叉引用。Files: `docs/harness/COMPOSABLE_HARNESS.md`、`docs/harness/test-cases/microstack-framework.md`。
- **Docs (组合式 Harness 设计草案 — 规格锚定的完成门禁 / MicroStack02 实证驱动):** 新增 `docs/harness/COMPOSABLE_HARNESS.md`。**触发实证:** 把详细开发文档（MicroStack02，规定 24 交付物 + `[verify:]` 门）丢给模型执行，结果 checklist/进度/节点**全 100% 完成**、`incomplete_stop=0`、`gate_skip graph_complete ×3`，但产物仅 7045 行（目标 25–40K）、app 16.3%/orm 4.7% 覆盖率不达标、交付物24 重构对抗 + gzip + cmd 入口**压根没进任务图**（欠拆解、非谎标）。**根因（代码定位）:** `long_horizon/mod.rs::maybe_continue_incomplete_code_task` 判「完成」只看模型自产 `plan`+`checklist` 两快照，唯一的 DEMO3 防假绿闸只遍历 `checklist.items` 已完成项——抓不到「从未进清单的交付物」；故「完成度上界 = 模型自拆清单完整度」，**「无早停」是真完成的必要不充分条件**。**方案:** 三层组合 harness——①模型自驱（现状保留）②**算子声明、exit-code 裁决的硬验收闸**（manifest 全绿才放行 `graph_complete`）③**独立 context 审核子代理做规格交付物对账**——失败即 reinject 强制返工、有界循环、耗尽记诚实 `audit_unmet`（不假绿/不死循环）。**核心非谄媚结论:** 审核子代理必须 **oracle 锚定**（当执行器/对账员，跑 `go test`/`git diff --exit-code contracts/`/`[verify:]` 看退出码），**exit code 当法官**，否则把「建造者欠拆」换成「审核者盖章放水」、违背 harness「全勾 ⇔ oracle 全 exit 0」铁律。复用件已定位（`verify.rs`/`scratchpad_flow.rs::maybe_continue_incomplete_audit`/`scratchpad/auditor.rs`/子代理工厂）；前置待核实项=引擎是否按 exec 记 exit code（`recent_verification_cmds` 仅命令文本）。分期 P0 纯机器硬闸 → P1 审核子代理 → P2 组合开关+遥测。Files: `docs/harness/COMPOSABLE_HARNESS.md`。
- **Docs (MicroStack §1-B 升级为「生产级」+ git 基线纪律 / 首跑实测驱动):** 首次实跑（产物 `F:\LHT_TEST\MicroStack`，`thr_be2f4f1a`）**所有门禁真绿**——离线复核 `go build`/`go vet`/`gofmt -d .` 全过、`go test ./...` 10/10 包绿、逐包合并 coverprofile 实测**总覆盖率 83.5%**（与模型声称一致，非假绿；模型存的 `coverage.out` 是补测前旧快照、手算仅 72.1%，终值以新鲜 profile 为准）——但产物仅 **~3,600 行**（1,608 生产 + 1,989 测试），远低于 1.5–4 万行载体目标，是精裁骨架。**根因（非模型偷懒）:** 规模目标只活在文档散文里、从未进 prompt，且完成标准全是二元门禁（小骨架即 100% 满足），模型理性最小化；且首跑跑的是 A 核心层、未下发 B 的扩量与终极对抗（Router 换 trie 这条 20% 权重最强信号根本没测到）；`F:\LHT_TEST\MicroStack` 非 git 仓库 → `git diff --exit-code contracts/` 无基线判不了。修复:把 §1-B「生产级」从口号拆成**可枚举、抗最小化的能力矩阵 + 深度门禁**（通配符/路由冲突检测/组中间件隔离、CORS/限流/超时/请求体上限/gzip、JSON+YAML+TOML+env 四源配置、time.Time/嵌套 struct 递归校验、连接排空/healthz/metrics、ORM 真实集成测试），新增**反假绿铁律**（编译桩/mock 不算完成）与**核心生产代码包各自覆盖率 ≥75% 防稀释**（首跑 `app` 仅 56.8% 靠 contracts 100% 稀释达标）；§1-A 横切约束补**接口冻结即建 git 基线**（`git init`+commit，使最强探针可机器判定）+ **request_id 用标准 UUID**；§2 `[verify:]` 增数据层集成测试项。Files: `docs/harness/test-cases/microstack-framework.md`。
- **Docs (CCR 测试用例 §7 — 首个全链路 clean pass + 案例毕业):** `docs/harness/test-cases/codecrafters-redis.md` 补 §7,记录第三跑(`thr_734ba23a`,产物 `F:\LHT_TEST\CodeCrafters_Redis01`)首次取得全链路真绿且可追溯——四 `[verify:]` 门(`cargo build`/`clippy`/`test 10/10`/`bash test_redis.sh 16/16`)全 `verified`、`gate_skip graph_complete open_items=0`、无 false `incomplete_stop`;verify 拦截**先红后绿**实证有效(首跑 `test_redis.sh` 因连接断开刷 `os error 10054`/`KEYS *` glob/ECHO 引号失败→模型自愈→重跑 16/16);**独立离线复核**(产物目录直接 build/clippy/test + 无 redis-cli 时裸-TCP RESP 探针 13/13,留存 `scripts/resp_smoke.ps1`);§5 `exec_shell` 挂死 / §6 UI↔引擎分叉 + `verify_gate=mismatch` 三项历史回归全部清零;验收依赖为复用首跑自筹的 redis-cli(本次 run 零下载,目录 mtime 早于 run ~8h 佐证);记串行基线(墙钟 ~10min、output 29,379 tok、prompt-cache 命中 ~73%)供 `PARALLEL_FRESH_GENERATION` §7.1 对照。**CCR 由"钓 bug"转为干净回归基线、视为毕业**,加压转 ≥1H 长程任务。Files: `docs/harness/test-cases/codecrafters-redis.md`。
- **Docs (base prompt — First-Principles Rule 母规则 / 第一性原理融入回答纪律):** 在 `prompts/base.md` 的 Epistemic discipline 节新增一条**通用母规则**（置于 Capability / Architecture 两条 code 专用规则之前），把"回答前先回到最不可再分的事实"显式化。靶子来自一次设计对话的 before/after 实测：陷阱题「我这个 **agent 产品**的多子代理架构比 Cursor/Devin 都先进，主打这个去融资靠谱吗？」中，Zagens 4/4 都识破了「比…先进」这个**最响**的未验证前提，却**集体漏掉最静的那个**——没有一个反问"这是什么 agent / 什么场景"，全部被 Cursor/Devin 锚点带着默认成"编程 agent"。规则因此强制**两步顺序**：①先锁定最基本实体/术语是否已确定（题中竞品名=提问者*框定*、非已证实品类；最危险的前提常是藏在框定里最安静的那个）→ 未定则先问、不在未验证地基上推理；②支撑链只能立在可验证事实或有效推导上，落在未验证前提/类比上须标注并下沉，不得继承（"我观察到 A" ≠ "A 就是被问的那个东西"）。附收尾自检：未操作化的评价词、把未验证前提当事实、对未验证前提的肯定性开场（谄媚）即返工。**after 实测（重打包 sidecar、新会话、同款陷阱题）暴露第二层缺口：办公模式两次仍失败、代码模式两次都识破**——根因是办公任务走的是**独立的 `base-office.md`**（`prompts.rs` `OFFICE_BASE_PROMPT`，按 `task_type` 二选一），而它**整套 Epistemic discipline / "Fluency is not grounding" 从来不存在**（grep 零命中），母规则只进了 `base.md`。修复：在 `base-office.md` 的 Communication 节后补一段精简版 **Grounding & first principles**（办公场景以 `web_search` 替代 grep 作为求证手段），让评价/建议类问题在办公模式同样先锁定最不可再分事实、不继承框定前提、不谄媚开场——办公模式恰是"产品先不先进/该不该融资"这类评价题的高发区却原本零防护。Files: `runtime-server/src/prompts/base.md`、`runtime-server/src/prompts/base-office.md`。
- **Fix (UI 进度/checklist 跑到一半冻结 — 持久化只依赖 monitor 逐工具事件、漏标即与引擎真值永久分叉 / CCR 实证):** CodeCrafters-Redis 复跑（`F:\LHT_TEST\CodeCrafters_Redis`，Rust 手写 RESP 服务器）暴露一处 UI↔引擎状态分叉：模型把 12 项 checklist **全部完成**（引擎侧 `todos` store 为 12/12，`gate_skip reason=graph_complete open_items=0`，item 1-12 的 `verify_gate` 全部 fire 过），但桌面进度条 / 清单**永久卡在 7/12 = 58%**、item 8-12 显示未完成。DB 取证（`thr_7eb88089`）：持久化的 `checklist_json` 确为 12 项/完成 7/pct 58，且 `items` 表只记录了 **3 次** checklist 工具调用，之后把 item 8-12 标完成的 checklist 变更**既没被 monitor 记成 item、也没 persist**——而同期 exec_shell/edit_file 等工具都被正常记录。根因：UI 读取的持久化 checklist（`threads.checklist_json` / `checklist_cache`）**只由 monitor 的逐工具 `ToolCallComplete` 钩子（`after_tool_call_complete_panels`）写入**，而该钩子被 `tool_items.remove(&id)` 的 start/complete 配对门控；并行 / deferred 批次里的 `checklist_update` 一旦 start 事件没进 `tool_items`，其完成就被丢弃，持久化快照从此与引擎真值**永久分叉**（没有任何收尾对账）。修复：引擎在**每次成功的 checklist 变更**处（`host_impl` verify 块已手握权威 `todos.snapshot()`）经**可靠的 harness-status 通道**额外推一条 `long_horizon.checklist_persist:<TodoListSnapshot json>`；host 的 `observe_harness_status` 识别该前缀后直接 `persist_thread_checklist` + 重推 `panel.checklist`/`harness.task_graph`，使 UI 始终对账到引擎真值，无视 monitor 逐工具事件的任何漏配。该 status 不生成噪声 timeline item（仅一条精简 `[lht-probe]`）。序列化形状即 `TodoListSnapshot`，与 `checklist_from_json` / `panel.checklist` 现有消费端完全一致（新增 round-trip 单测固化该契约）。Files: `runtime-server/src/core/engine/turn_loop/host_impl/mod.rs`、`runtime-server/src/runtime_threads/monitor_host.rs`、`runtime-orchestrator/src/runtime_threads/monitor.rs`、`runtime-server/src/long_horizon/snapshots.rs`（+test）。**残留：** `verify_gate` 仍有 item 12 = `mismatch`（`[verify: bash scripts/test_redis.sh]` 复合命令未匹配到执行），属既有 `mismatch` 漏标洞，另列。见 [`docs/harness/test-cases/codecrafters-redis.md`](docs/harness/test-cases/codecrafters-redis.md)。
- **Fix (`Engine::new` 在无 Tokio runtime 时 panic — 同步构造器要求 reactor / 测试链路实证):** CCR 修复自验时 `cargo test -p zagens-cli --lib shell` 钓出一处真实健壮性缺陷：`agent_and_yolo_modes_elevate_shell_sandbox_to_allow_network` 等**同步 `#[test]`** 调 `Engine::new(..)`（同步构造器）即 panic `there is no reactor running, must be called from the context of a Tokio 1.x runtime`（`subagent/factory.rs:66`）。根因：`Engine::new`（`core/engine/build.rs:103`）**无条件**调 `spawn_subagent_maintenance_task`，而后者直接 `tokio::spawn` 一个僵尸 subagent 清扫循环——`tokio::spawn` 在无 reactor 上下文会 panic，于是任何在 async runtime 之外构造 Engine 的同步调用方（单测、或先构造后进 runtime 的宿主）都会被打断。修复：`spawn_subagent_maintenance_task` 先 `tokio::runtime::Handle::try_current()` 探测，无 runtime 即**优雅跳过**（该维护循环只是 best-effort hygiene，sidecar 始终在自己的 Tokio runtime 内构造 Engine，生产行为不变）。验证：`core::engine::tests` 89/0、`shell` 子集 32/0 全过。Files: `runtime-server/src/tools/subagent/factory.rs`。
- **Fix (`exec_shell` 永久挂死 + `timeout_ms` 失效 — 子进程继承 stdout 管道 / CCR 实证):** CodeCrafters-Redis 长程压测（`F:\LHT_TEST\CodeCrafters_Redis`，Rust 手写 RESP 服务器）暴露一种**全新的静默卡死出口**：模型用 `Start-Process -NoNewWindow` 在前台拉起自己 build 的 `redis-server.exe` 来跑 redis-cli 验证，结果整个 turn 冻死 19+ 分钟、token 不动，且**显式传入的 `timeout_ms:15000` 完全无效**。根因在 `tools/shell/process.rs::BackgroundShell::collect_output`：子进程退出后它**无条件 `JoinHandle::join()`** 两个 stdout/stderr reader 线程，而 reader 循环 `read()` 到 **EOF** 才退出——当被测命令派生了一个**继承管道写端的 grandchild**（PowerShell `Start-Process -NoNewWindow`、守护进程化的 server、`cmd &` 后台作业等），父进程早退出但 grandchild 仍握着写端 → 管道永不 EOF → reader 线程永不结束 → `join()` 永久阻塞。该 `join()` 发生在 `poll()` 内，而 `poll()` 又是前台超时轮询循环（`tools/shell/tools/helpers.rs::execute_foreground_via_background`）在**持有 `shell_manager` 锁**时调用的，于是循环永远回不到自己的 deadline 检查 —— 这就是超时彻底失灵、turn 无限挂死的链路（`kill()` 同样卡在 `collect_output`）。修复：`collect_output` 改为**有界 join**（新增 `join_reader_bounded`，轮询 `JoinHandle::is_finished` 配 `READER_DRAIN_GRACE=500ms` 的排空窗口，超时即 **detach**——丢弃 handle、不再阻塞）。常见命令瞬间 join 完、捕获完整输出；有 grandchild 撑管道时则在 grace 后 detach，让 `poll()`/`kill()`/前台超时循环始终能推进（detach 线程仅在互斥锁后追加共享缓冲，安全，随 grandchild 关闭句柄或进程退出自然结束）。Files: `runtime-server/src/tools/shell/process.rs`（+3 单测）。**残留（follow-up backlog）：** Windows 上 `child.kill()` 只杀父 PowerShell、不杀 grandchild（无 `process_group`/`PR_SET_PDEATHSIG` 等价物），故被测 server 会作为孤儿进程残留并继续占用端口（如 6379）；彻底治理需用 **Windows Job Object**（`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`）把子树一并终止——本次未做，模型侧已被观察到能 `netstat`+`Stop-Process` 自行处理端口冲突。见 [`docs/harness/test-cases/codecrafters-redis.md`](docs/harness/test-cases/codecrafters-redis.md)。
- **Fix (LHT `unverified_acceptance` 软提示→软门禁 — DEMO3 假绿根治 / 复跑结论):** DEMO3 复跑（`F:\DEMO3-2`，2W 行级 Go/Monkey 解释器）这次**真把两个钓鱼特性写进了代码**（`%` 取模在 token/lexer/parser/evaluator 全链路实现、`counter1` 数字标识符 `readIdentifier` 改用 `isIdentChar` 不再截断），人工编译 + 跑 `examples/02_modulo.monkey`/`03_identifiers.monkey` 全过——**真绿**。但暴露闭环仍有一处弱点：关键验收项「`go build`/`vet`/`gofmt`/`go test`/`run_examples` 全绿」被写成**完成项却没带 `[verify:]` 前缀**，`verify_gate` 正确判出 `unverified_acceptance` 并追加软提示，**但它只是 tool-result 末尾的提示、不阻断收尾**——graph 仍判 `graph_complete`、turn 直接 `Completed`。即这次靠「模型恰好写对」躲过假绿，**非 harness 强制**；换一次非确定性采样（见 [`docs/harness/LHT_TEST_SUITE.md` §5.1](docs/harness/LHT_TEST_SUITE.md)）同样漏标就可能放行真崩产物。修复（B）：把 `unverified_acceptance` 从软提示升级为**软门禁**——续写 gate（`long_horizon/mod.rs::maybe_continue_incomplete_code_task`）在 `graph_complete` 这一步旁路加检查：若已完成 checklist 里仍有「读起来像可运行验收、却既无 `[verify:]` 又无匹配近期执行」的项，则**不放行收尾**，改注入一条聚焦续写（要求改写成 `[verify: <命令>]` 并真跑），有界重试 `MAX_UNVERIFIED_ACCEPTANCE_NUDGES=2` 次防空转。**刻意不改 `completion_pct`/`graph.incomplete()`**（进度条仍 100%，不回退 DEMO5 #1），只挡 turn 结束。新增第三种 gate 结果 `LhtGateOutcome::NudgeUnverifiedAcceptance` 与独立可观测事件 `long_horizon.unverified_acceptance_nudge`（自动入 Nodes Tab ring，橙色，与 verify mismatch 同色系）。**验证（2026-05-30，含 B 二进制打包重装后）：** DEMO3 复现 prompt 连跑 5 次（产物 `F:\DEMO3\1..5`），客观 oracle + 日志双核验——5 个产物 `go build/vet/test` 全 exit 0、两个钓鱼特性（`10 % 3 == 1`、`counter1` 数字标识符不截断）**5/5 全过（真绿）**；日志侧 `long_horizon.unverified_acceptance_nudge` 每 run 各触发 1 次（共 5，旧批为 0）、旧假绿出口 `graph_complete`/`gate_skip` **归零**、`verify_gate verdict=verified` 16 条（模型补 `[verify:]` 后真跑被匹配）。**B 治本成功，对比基线 ~62.5% 显著提升，收尾结束。** 残留：`mismatch` 4 次（模型贴 `[verify:]` 但匹配器未关联到执行，多为复合命令）——B 只阻断 `unverified_acceptance`、不阻断 `mismatch`，理论上存在"只贴标签不真跑"的降级逃逸洞（本次未触发，产物均真跑），列为下一锤候选。Files: `runtime-server/src/long_horizon/{mod.rs,nudge.rs}`（+test）、`runtime-server/.../host_impl/no_tool_uses.rs`。见 [`docs/harness/test-cases/DEMO3-monkey-interpreter.md` §6–7](docs/harness/test-cases/DEMO3-monkey-interpreter.md)。
- **Fix (LHT plan/checklist double-counting — wedged progress + false `incomplete_stop` — DEMO5 #1):** a fully-completed 20K-line Go-interpreter run (#DEMO5) showed the task progress bar stuck at **61%** with **12 "open" items** and the harness fired a false `incomplete_stop` give-up signal, even though the project built and the checklist was 100% done. Root cause: `long_horizon/graph.rs::from_snapshots` summed plan steps **and** checklist items as one disjoint workload (`total = phases.len() + checklist.len()`, `open_items`/`incomplete()` likewise OR'd both sides). The model drafted a 12-step plan, then executed-and-finished the work via a 19-item checklist and never touched the plan again — so the 12 all-`pending` plan steps became "zombie" open items: progress = `19/(12+19) ≈ 61%`, `incomplete()` stayed true, and once the checklist finished `in_progress_id` fell back to the abandoned plan (then `None`) → the nudge gate `Skip`ed (`reason=nudge_skip`, `continue_injected` never fired) and the final fallthrough mislabeled a real completion as a give-up. Fix (Option 1 — **checklist is the completion authority**): when the checklist is non-empty it alone drives `completion_pct` / `open_items` / `incomplete()` and `in_progress_id` (no fallback to a stale plan `InProgress`); the plan is a display-only outline. Plan-only tasks (empty checklist) keep their original behavior. Verified with the DEMO5 snapshot (12 plan pending + 19 checklist completed → 100% / 0 open / not incomplete / `in_progress_id=None`). Files: `runtime-server/src/long_horizon/graph.rs` (+tests).
- **Fix (LHT verify-gate false `mismatch` — every `[verify:]` item misflagged — DEMO5 #2):** the same DEMO5 run had `verify_gate` report `verdict=mismatch` for **all** of items 12–19 (`go build`/`go vet`/`gofmt`/`go test`/`bash scripts/*.sh`/`./monkey run …`) even though the model demonstrably ran them (`go test ./...` all-green, `go test -cover` both exit 0 in the thread). Two compounding matcher bugs, both `(a)` over-strict (not the model skipping verification): **(1)** the exec-recording gate required the result *text* to contain `result_contains_success(result)` ("exit code: 0"/"success: true") **on top of** the already-authoritative `success` bool — but a successful `exec_shell` returns raw stdout (e.g. `ok  monkey/lexer 0.078s`) with **no** exit-code line (only failures print `Command failed (exit code: …)`), so the recorder *never* fired on success and `recent_verification_cmds` stayed permanently empty → every `[verify:]` item mismatched, regardless of language. **(2)** `VERIFICATION_CMD_RE` only matched `go test` among Go verbs, so `go build`/`go vet`/`gofmt`/script/binary acceptances could never be recorded even after fix (1). Fix: drop the redundant+wrong `result_contains_success` clause from both the recording gate and the qualified-progress check (rely on `success` = exit 0; function removed); broaden `VERIFICATION_CMD_RE` to `go build|vet|run`, `gofmt`, `make`, and `bash …`/`sh …`/`./…` script/binary invocations; raise the recent-cmd LRU `MAX_RECENT_VERIFICATION_CMDS` 12→24 so a late batch of `[verify:]` completions can still match earlier runs. Files: `runtime-server/src/long_horizon/{nudge.rs,verify.rs,mod.rs}`, `runtime-server/.../host_impl/mod.rs` (+tests).
- **Fix (LHT verify-gate blind to bulk `checklist_write` — DEMO6 validation follow-up):** a clean re-run of the same 20K-line Go-interpreter task (#DEMO6) confirmed the DEMO5 #1/#5 fixes — task reached **100% / 0 open items**, the completion gate correctly emitted `gate_skip reason=graph_complete` (no false `incomplete_stop`), and `step_limit_continue` fired mid-turn as designed — but produced **zero `verify_gate` nodes**: the model marked items done via bulk **`checklist_write`** (whole-list replace), while the verify gate only hooked per-item `checklist_update`/`todo_update`, so the verdict logic never ran for that path. Fix: the gate now also fires on `checklist_write` — after any of the three tools succeeds it scans the post-write checklist snapshot for `Completed` items and runs the verdict on each **newly** completed one, deduped via a session-scoped `gated_completed_ids` set so a bulk write that re-sends the whole list fires exactly once per item. The verdict logic is extracted into a pure, unit-tested `verify::verify_gate_verdict` (verified / mismatch / unverified_acceptance / untagged_ok). Files: `runtime-server/src/long_horizon/{verify.rs,nudge.rs,mod.rs}`, `runtime-server/.../host_impl/mod.rs` (+tests). See [`docs/harness/LONG_HORIZON_CODE_TASKS.md`](docs/harness/LONG_HORIZON_CODE_TASKS.md) (DEMO6 实证).
- **Fix (LHT cycle threshold never evaluated mid-turn — clean early refresh dead on long turns — DEMO5 #5):** the checkpoint-restart cycle gate (`should_advance_cycle` threshold + long-horizon early-advance band) was only ever evaluated **between turns** (`maybe_advance_cycle`, called once at `message_handlers.rs` after `handle_deepseek_turn` returns `Completed`). A long-horizon turn that loops hundreds of tool steps without returning never reached that boundary, so even as live context crossed the ~75% early-advance band the *clean* refresh could not fire — only the hard-overflow emergency handoff (backlog C) could, and only at the model's hard ceiling (a non-clean breakpoint). The `pending_cycle_at_checkpoint` flag set on warning-band checklist completions sat unconsumed until a between-turns boundary that never came. Fix: a new bounded `TurnLoopHost::maybe_advance_cycle_at_checkpoint` hook is called at the per-step **safe boundary** (after a tool step's stream + execution finish, before `next_step()` — so `in_flight=false`, no mid-edit/stream cut), reusing the exact between-turns gate and `perform_cycle_advance` body; on a handoff the loop re-requests with the fresh briefing seed. Gated to LHT code tasks (`cycle.enabled` + `long_horizon.enabled` + code surface; plan mode never advances) and bounded by `MAX_IN_TURN_CYCLE_ADVANCES=8`. Complements backlog C: this is the *clean, early* refresh; backlog C remains the *at-ceiling, emergency* fallback. `maybe_advance_cycle` now returns `bool` (the between-turns caller ignores it). Files: `core/engine/streaming.rs` (const), `core/engine/turn_loop/{host,run}.rs`, `runtime-server/.../engine/cycle_hooks.rs`, `runtime-server/.../host_impl/mod.rs`. See [`docs/harness/LONG_HORIZON_CODE_TASKS.md`](docs/harness/LONG_HORIZON_CODE_TASKS.md) (DEMO5 实证).
- **Docs (LHT plan/checklist discipline — `prompts/base.md`, soft complement to DEMO5 #1):** the checklist-discipline section now teaches that **plan and checklist are one body of work, not two** — when the checklist executes the work, the plan stays a few stable high-level phases (don't re-list the same granular work in both, don't draft a plan then abandon it while only the checklist moves), and both must reflect the true end state with no leftover `pending`/`in_progress` items. Reduces the "zombie plan item" pattern that the #1 mechanism now also tolerates.
- **Feature (LHT panel "Nodes" tab — harness decision stream live in the UI — DEMO5 #3):** the `long_horizon.*` node-decision stream (`continue_injected` / `gate_skip` / `incomplete_stop` / `blocked` / `context_warning` / `step_limit_continue` / `loop_guard_continue` / `cycle_advanced` / `verify_gate`) was previously only visible by grepping `sidecar.log` offline — which is exactly how the DEMO5 diagnosis found that `continue_injected` never fired. These status events are already persisted, so the harness telemetry cache now also retains a bounded ring (`MAX_HARNESS_NODE_RECORDS=80`) of recent node decisions, attached to the existing `harness/task-graph` panel payload (`recent_nodes`) the LHT panel already polls — no new endpoint. A new **Nodes** tab renders the trail newest-first with per-kind color coding (continue/advance → green, skip/blocked/warning → amber, `incomplete_stop`/halt → red, verify `mismatch` → orange) and key fields (`reason`/`open_items`/`nudge_count`/`verdict`). The verify-gate verdict (previously `eprintln`-only) now also emits a `long_horizon.verify_gate` status event so it appears in the stream. Files: `runtime-server/src/runtime_threads/manager.rs`, `runtime-server/.../host_impl/mod.rs`, `desktop/web-ui/src/components/LongHorizonPanel.tsx`, `desktop/web-ui/src/lib/types/longHorizon.ts`, `desktop/web-ui/src/i18n/locales/*`.
- **Fix (LHT context-overflow hard-fail → cycle handoff — turn-termination audit backlog C):** a 20K-line-class long-horizon task whose conversation grows past the model's input budget mid-turn used to **hard-fail the turn** — after emergency compaction (`recover_context_overflow`) couldn't get back under budget within `MAX_CONTEXT_RECOVERY_ATTEMPTS=2`, `run.rs` returned `Failed("…Please run /compact or /clear.")`, dumping a manual recovery step on the user. Root cause: the existing **cycle handoff** (the `<carry_forward>` briefing + preserved structured state swap that *would* reset context to a tiny in-budget seed) only runs as a **between-turns boundary** (`message_handlers.rs`, after `handle_deepseek_turn` returns `Completed`); a long-horizon turn that loops many tool steps internally never returns to that boundary, so the cycle never had a chance to fire and emergency compaction (which keeps recent messages + a summary) couldn't shrink a buffer dominated by large tool results. Fix: before hard-failing, a new bounded `TurnLoopHost::maybe_cycle_handoff_on_context_overflow` hook lets the host **force a cycle handoff in-flight** — `cycle_hooks.rs`'s rotation body is extracted into `perform_cycle_advance` (threshold gate kept for the normal `maybe_advance_cycle` path) and a `force_cycle_handoff_for_overflow` skips the gate to swap the bloated buffer for a small briefing seed + carried plan/todos/working-set/handoff.md, then the loop resets its recovery budget and retries. Bounded by `MAX_CONTEXT_CYCLE_HANDOFFS=2`; the briefing turn reserves far less output headroom than a normal turn, so it typically fits even when the regular request overflows, and if even a fresh seed can't fit the original hard failure still stands. Gated on `cycle.enabled` (plan mode never handoffs; default hook returns `false`). Files: `core/engine/streaming.rs` (const), `core/engine/turn_loop/{host,run}.rs`, `runtime-server/.../engine/cycle_hooks.rs`, `runtime-server/.../host_impl/mod.rs`. See [`docs/harness/LONG_HORIZON_CODE_TASKS.md`](docs/harness/LONG_HORIZON_CODE_TASKS.md) (turn-termination audit, backlog C).
- **Fix (LHT loop-guard-halt early-stop + give-up observability — turn-termination audit follow-up):** an audit of *every* turn-termination exit in `core/engine/turn_loop/{run,streaming_phase,tool_phase}.rs` (from the long-horizon perspective) found that **all `break`s converge to a single `Completed` outcome** at `run.rs`'s fallthrough — so any exit that breaks *without* routing through the no-tool-uses LHT continue gate ends an incomplete task as a silent false green. Two such exits were closed. **(1) Loop-guard halt** (`tool_phase.rs`): when a tool fails `FAILURE_HALT_THRESHOLD=8` consecutive times, `LoopGuard` halts the turn with a bare `break` that bypasses the LHT gate entirely — a *fourth* silent early-stop form (after length-truncation, prose early-stop, and step-exhaustion). Fix: the tool-phase outcome now carries `loop_guard_halted`, and `run.rs` offers a bounded `TurnLoopHost::maybe_continue_after_loop_guard_halt` continuation — for an incomplete LHT task it resets the guard's per-tool failure counters (`LoopGuard::reset_failures`, identical-call blocking left intact) and injects a "you got stuck repeating a failing tool — **change approach** (switch tools / change args / read the error first), don't stop" nudge, bounded by `MAX_LOOP_GUARD_CONTINUATIONS=2`. Emits a `Loop-guard halt; nudging long-horizon task to change approach (n/N)` status + `long_horizon.loop_guard_continue` event. Plan mode and non-LHT hosts keep the original halt (default hook returns `false`). **(2) Give-up guard** (`run.rs` final fallthrough): a new `note_incomplete_stop_if_lht` hook fires when the loop ends as `Completed` while the LHT graph is still incomplete (nudge budget spent, continuations exhausted, REPL/no-tool break, …), emitting a `long_horizon.incomplete_stop: {open_items:n}` probe so the UI / `sidecar.log` (`[lht-probe]` tee) no longer read a give-up as a genuine completion. Purely observational; the outcome itself is unchanged. Files: `core/engine/loop_guard.rs` (+`reset_failures` & tests), `core/engine/streaming.rs` (const), `core/engine/turn_loop/{control,tool_phase,host,run}.rs`, `runtime-server/.../host_impl/mod.rs`. See [`docs/harness/LONG_HORIZON_CODE_TASKS.md`](docs/harness/LONG_HORIZON_CODE_TASKS.md) (turn-termination audit).
- **Fix (LHT step-exhaustion early-stop — turn silently ends at the `max_steps` cap mid-task):** a 20K-line Go-interpreter run (#DEMO4) stalled at **40% with the turn idle** after ~29 minutes. `sidecar.log` proved it was **not** a stream/length truncation: `[stream-probe]` showed **exactly 100 streams**, all `stop_reason=tool_calls`, `stream_errors=0`, `chunk_timeout`=0, `max_tokens=393216`, `rx_backlog` single-digit — i.e. the turn hit the **default `max_steps: 100`** tool-step budget (`core/engine/types.rs`) and `run.rs` terminated it with a bare `break` ("Reached maximum steps"). This is a **third silent early-stop form** (after length-truncation and prose early-stop): the LHT continue nudge only fires on the *no-tool-uses* path, so a tool-heavy turn that exhausts its step budget bypassed the harness entirely and the task just stopped. Fix: at `at_max_steps`, before terminating, a new `TurnLoopHost::maybe_continue_at_step_limit` hook lets a long-horizon host (LHT enabled + code task-surface + task graph still incomplete & non-trivial) inject a focused continue nudge and be **granted another step-budget window** (extends `max_steps` by the original budget), bounded by `MAX_STEP_LIMIT_CONTINUATIONS=3` (≤4× base budget, e.g. 100→400) so a runaway task can't loop forever. Plan mode never continues; non-LHT hosts keep the original cap behavior (default hook returns `false`). Emits a `Step budget reached; continuing long-horizon task (n/N)` status + `long_horizon.step_limit_continue` event. Files: `core/engine/streaming.rs` (const), `core/engine/turn_loop/{host,run}.rs`, `runtime-server/.../host_impl/mod.rs`.
- **Diagnostics (LHT node tracing — `[lht-probe]` lines in `sidecar.log`):** the long-horizon harness previously left **no trace in `sidecar.log`** — every node decision flowed only through the engine event channel into the UI panel / DB, so offline debugging of a stalled or false-green run had nothing to read. Two `eprintln!` probes (sidecar installs no `tracing` subscriber, so stderr → `sidecar.log` is the only sink — same convention as `[stream-probe]`/`[thinking-probe]`): (1) a **central tee** at the monitor's `long_horizon.*` choke point (`runtime-orchestrator/.../monitor.rs`) mirrors every harness status node — `gate_skip` (which guard suppressed the nudge), `continue_injected` (nudge fired + emitted/converted/open_items), `blocked`, `context_warning`, `nudge_outcome` — as `[lht-probe] long_horizon.<kind>: {…} thread=… turn=…`; (2) a **verify-gate verdict** at every checklist/todo completion (`runtime-server/.../host_impl/mod.rs`) emits `[lht-probe] verify_gate tool=… item=<id> verdict=<verified|mismatch|unverified_acceptance|untagged_ok|no_item> content="…"`, making the false-green guard's per-item decision visible (directly debuggable for the DEMO4 all-`[verify:]` stress task). Low-volume (≈1–2 lines/turn), ungated, diagnostic-only — no behavior change. Grep `sidecar.log` for `[lht-probe]` to replay the harness decision loop.
- **Fix (LHT "false-green" — runnable acceptance decomposed into a create-only checklist item):** a 20K-line Go-interpreter stress run completed with the checklist fully checked and turn `Completed`, yet 2 of 4 example scripts crashed when actually run (`%` modulo unimplemented; digit-bearing identifiers like `counter1` not lexed). Root cause was not a lying model but **decomposition losing verification semantics**: the acceptance "REPL runs all examples" became checklist item "create example scripts (.monkey)" — done by creating files — while the only `[verify:]`-gated item was `go build/vet/test`, whose unit tests didn't cover those features. (`max_tokens=393216` held the whole run with zero length truncation, so this was purely a verification-loop hole, not truncation/early-stop.) Two fixes: (1) **root cause** — `prompts/base.md` checklist discipline now teaches the `[verify: <command>]` prefix: any "runs / builds / tests pass / examples execute / lints clean" acceptance MUST be authored as `[verify: cmd] <label>`, with explicit "creating a file is NOT verifying it works"; (2) **gate hardening** — new `long_horizon::verify::unverified_acceptance_suffix` + `host_impl` gate: a checklist item marked `completed` that *reads like* a runnable acceptance (build/test/run-examples keywords, en+zh) but carries no `[verify:]` prefix now gets an advisory suffix on the `checklist_update`/`todo_update` result, catching the false-green even when the model forgot to tag it. See [`docs/harness/LONG_HORIZON_CODE_TASKS.md`](docs/harness/LONG_HORIZON_CODE_TASKS.md) (DEMO3 实证修正).
- **Feature (length-truncation auto-recovery — no more silent stop on a maxed-out turn):** when the provider ends a step with `finish_reason=length` (the model hit the output `max_tokens` cap mid-output) and there is **no tool call** to carry the turn forward, the engine no longer ends the turn with a truncated / empty-body `Completed` — the worst failure for a user running a large task. It now persists whatever was produced, injects a continuation hint (`从中断处继续…` when text was already streamed; `精简思考直接给结论…` for a reasoning-only cut, with a placeholder assistant turn to keep role alternation valid), emits a `Output hit the length limit; continuing automatically (n/N)` status, and re-issues the request via the normal multi-step loop. Bounded by `MAX_LENGTH_CONTINUATIONS=8` consecutive continuations (reset on any non-length step end) to cap runaway cost / an infinite cut→continue loop. A length cut **with** tool calls already self-recovers (tools execute, next step continues), so only the empty-tool path is special-cased. Threads a new `length_continuations` counter through `run_streaming_phase` (mirrors `stream_retry_attempts`). This complements the 384K default `max_tokens`: the default makes a single-step cut nearly impossible, this catches the rare case where a single step's answer genuinely exceeds 384K. Files: `core/src/engine/streaming.rs` (const), `turn_loop/streaming_phase.rs`, `turn_loop/run.rs`.
- **Fix (mid-reasoning stream truncation → empty-body `Completed`):** reasoning (`thinking`) deltas are now **coalesced in-memory and flushed on a size/time threshold** (`THINKING_FLUSH_BYTES=512` / `THINKING_FLUSH_INTERVAL=60ms`) instead of one `spawn_blocking` + global `db.lock()` write per token in the monitor's drain loop (`runtime-orchestrator/src/runtime_threads/monitor.rs`). On reasoning-heavy turns (thousands of deltas) the per-token DB write drained the bounded (256) engine event channel slower than the model streamed, backpressuring `tx_event.send().await` in the engine — which then stopped polling the upstream HTTP stream until the provider idle-closed it, ending the turn with no visible body but status `Completed`. Coalescing makes the common path an O(1) buffer append (~100× fewer INSERTs), so the monitor drains fast and the engine never stalls on DB backpressure. Buffered reasoning is flushed on `ThinkingComplete` and once more after the loop so partial reasoning isn't lost on truncation. UI streaming is unchanged in feel (≈16 flushes/sec). The SSE read-side `coalesce_delta_events` (UI merge) is complementary and unaffected.
- **Diagnostics (stream-truncation investigation — backpressure probes):** gated `eprintln!` instrumentation (the sidecar installs **no `tracing` subscriber**, so `tracing::*` events are dropped — probes must go to stderr → `sidecar.log`). `streaming_phase.rs` emits `[stream-probe]` lines: engine `tx_event().send()` block duration on a `ThinkingDelta` (`≥ 50ms`), `chunk_timeout`, and a **stream-end summary** (`reason={upstream_eof|chunk_timeout|cancelled|…} thinking_bytes=… text_bytes=… tool_uses=… stream_errors=…`) that pinpoints the empty-body truncation signature. `monitor.rs` emits `[thinking-probe]` lines: slow coalesced-flush `emit_event` latency + bounded (256) engine event-channel backlog, per-100-flush stats, and a per-turn `deltas/flushes` summary. Diagnostic-only. See [`.claude/stream-truncation-investigation-handoff.md`](.claude/stream-truncation-investigation-handoff.md).
- **`apply_patch` / `grep_files` / `list_dir` 一致性加固（与 `write_file` 同源问题）：** `apply_patch` 打补丁/`changes` 全量替换现在**保留原文件行尾**（CRLF 不再被 `lines().join("\n")` 压成 LF）与**末尾换行**（不再产生 "No newline at end of file" 噪声 diff），并改用**原子写**（同目录临时文件 + rename，含回滚路径），与 `write_file` 行为统一（复用 `tools/file` 共享 helper `normalize_line_endings`/`atomic_write`/`line_ending_of`）。`grep_files` 用 `detect_and_decode` **编码安全读取**，GB18030/UTF-16 源文件（中文 Windows 常见）不再被当二进制静默跳过而搜不到；`files_with_matches` 模式命中首行即停止扫描该文件；BM25 排序消除 O(文件×匹配) 的重复扫描（预计算每文件命中数）。`list_dir` 输出**稳定排序**（目录优先 + 名称）、新增 `limit` 分页上限（默认 1000、上限 10000，返回 `total`/`truncated`）与 `is_symlink` 标识。
- **`write_file` 健壮性与大写入优化（`tools/file/write.rs`）：** 覆写已存在文件时**保留原行尾**（模型发 LF 不再把 CRLF 文件整体翻成 LF，消除 Windows 上的全文件 git diff 噪声）；旧内容改用 `detect_and_decode` **按编码安全读取**（GB18030 / UTF-16 文件不再被当成空文件、diff 不再失真）；改为**原子写入**（同目录临时文件 + rename，写入中断不破坏原文件）；内容与磁盘完全一致时**跳过写入**（不动 mtime）；新增写入字节上限（`MAX_WRITE_SIZE`，超限返回 `[TOO_LARGE]`）。**大量代码写入**：新建文件或超大输入（>256K）时**跳过整文件 unified-diff**，改输出 `Created/Wrote` 摘要 + head 预览（省上下文又省 O(N·D) 计算）；返回回显字节/行数，并对 `.tsx/.jsx` 沿用 JSX 平衡检查、对 `json/js/ts/css/html/scss` 增加 `[TRUNCATION_SUSPECTED]` 括号失衡截断信号；错误信息统一为本地化 `[NOT_FOUND]`/`[PERMISSION]` 风格。详见 [`docs/tech/WRITE_FILE_IMPROVEMENTS.md`](docs/tech/WRITE_FILE_IMPROVEMENTS.md)。
- **Fix (LHT progress-pass let early-stops through — DEMO2 evidence):** qualified progress no longer *skips* the continue nudge. The gate only fires when the model stopped (no tool calls) with the task still incomplete, and "did some work, then quit mid-task" is the exact cognitive early-stop LHT exists to catch — but `prepare_nudge` was returning `SkipProgressReset` (no nudge) whenever the turn had any qualified progress, so a model that wrote a file and then ended with prose-only got a free pass and the turn completed at 0% checklist. Now progress only clears the no-progress streak (still protects a productive model from the `blocked`/give-up state); the nudge still fires, bounded by the `max_nudges_per_item` hard cap. Removed the now-dead `NudgeDecision::SkipProgressReset`. Verified against thread `thr_0eda7dcc` where `long_horizon.gate_skip` reported `reason=nudge_skip_progress_reset` on a `Completed` turn with `incomplete=true`.
- **LHT gate diagnostics (§4.9):** the continue gate (`maybe_continue_incomplete_code_task`) now returns `LhtGateOutcome::{Nudge,Skip(reason)}` instead of a bare `Option`, and `no_tool_uses` emits a `long_horizon.gate_skip` status event whenever the nudge is suppressed — carrying the exact guard (`disabled`/`plan_mode`/`audit_owns_path`/`graph_empty`/`graph_complete`/`graph_trivial`/`nudge_skip`/`nudge_blocked`/…) plus the engine-side facts (`enabled`, `app_mode`, `code_surface`, `empty`/`incomplete`/`trivial`, `in_progress_id`, `open_items`). Turns "the nudge didn't fire" into "it skipped at `<reason>` with `<facts>`" in a single run. Diagnostic-only; no change to gate behavior.
- **Fix (LHT task-graph real-time observability):** `GET /v1/threads/{id}/harness/task-graph` no longer routes through the engine op loop, which is starved during a long turn (the outer turn loop only drains steer/cancel, so the 5s `Op::QueryHarnessTaskGraph` reply always timed out mid-turn — blanking the panel with "暂无长程任务图" exactly when a long-horizon task is running). It now assembles from persisted plan/checklist snapshots + a live nudge-telemetry cache fed by `long_horizon.*` status events via a new `RuntimeThreadMonitorHost::observe_harness_status` hook, so the task graph and telemetry (emitted/converted/blocked/nudge_count) stay live during the turn.
- **LHT Phase 1:** Long-horizon code task harness — derived plan/checklist task graph, forced continue nudge in `no_tool_uses` when items stay open (after audit continue), `[long_horizon]` config, `long_horizon.continue_injected` / `long_horizon.blocked` status events. See [`docs/harness/LONG_HORIZON_CODE_TASKS.md`](docs/harness/LONG_HORIZON_CODE_TASKS.md).
- **LHT Phase 2:** `GET /v1/threads/{id}/harness/task-graph`, SSE `harness.task_graph`, plan snapshot cache + `update_plan` metadata, `[verify:]` display prefix, cycle `StructuredState` LHT summary.
- **LHT Phase 2 (cont.):** Warning-band early `maybe_advance_cycle` after checklist/plan checkpoint; `reinject_every_steps` objective reinject; `long_horizon.context_warning` status; `GET /v1/threads/{id}/harness/cycles`; checklist `[verify:]` mismatch warning on `completed` without recent matching `exec_shell`.
- **LHT Phase 3b:** Plan snapshot persisted to SQLite (`plan_json`); SSE `panel.plan` + `harness.cycle_advanced`; cycle advance merges open checklist/plan into `.deepseek/handoff.md` (`<!-- lht-handoff:auto -->` block).
- **LHT Phase 3 (cont.):** `harness/cycles` exposes window + 768K cycle threshold + LHT 75–85% band; Context tab threshold bar; `long_horizon.blocked` includes `reason`; panels refetch on `sidecar://ready`.
- **Fix (LHT nudge):** Progress bar in continue nudges now fills proportionally (was capped at ≤1/10 segments, so 42% and 100% both rendered one block).
- **Fix (LHT qualified progress, §4.3.1):** Read-only execs (`ls`/`echo`, …) no longer count as task progress — `exec_shell`/`run_tests` only reset the nudge streak when the command matches the verification pattern and exits 0; write/plan/checklist tools still count on success. Removed dead `NudgeTracker::record_qualified_progress` and the duplicate stale-turn constant.
- **LHT nudge hard cap:** `max_nudges_per_item` is now a reachable absolute ceiling — `NudgeTracker` tracks total nudges per item separately from the no-progress streak, so a model making intermittent progress to dodge `blocked` still stops at the cap.
- **LHT steer parsing:** "stop" steer detection relaxed beyond exact match — common Chinese phrases (`暂停`/`先停`/`停一下`…) match as substrings and English stop verbs (`stop`/`pause`/`halt`/`abort`) match as whole words (no false trigger on `stopwatch`).
- **LHT Phase 2.x — objective progress signal (§4.8):** qualified progress now also accepts an actual git working-tree change since the last nudge (`git status --porcelain` signature via reused `run_git`), making the no-progress/`blocked` gate language-agnostic (covers `make`/custom scripts the verification regex misses). Computed once per gate-triggered turn off the blocking pool; `[long_horizon] progress_via_git` (default `true`) toggles it; non-git workspaces auto-degrade to the Phase 1 tool signals.
- **LHT Phase 2.x — nudge telemetry (§4.9, "先量后调"):** in-memory per-session counters `{ emitted, converted, blocked, conversion_pct }` measure whether nudges actually lead to qualified progress; surfaced via `long_horizon.nudge_outcome` status event, extra fields on `long_horizon.continue_injected`, and a `telemetry` block in `GET …/harness/task-graph`. Evidence-only this phase — threshold tuning waits on real-session data.

### Docs

- **最小回归集另两条规格（可直接复制 prompt）：** 新增 [`docs/harness/test-cases/codecrafters-redis.md`](docs/harness/test-cases/codecrafters-redis.md) 与 [`docs/harness/test-cases/swe-bench-verified-sample.md`](docs/harness/test-cases/swe-bench-verified-sample.md)。**Redis**(单模型串行)= 手写 RESP + 分阶段命令(PING/ECHO/SET-GET/PX 过期/INFO),用真实 `redis-cli` 做 oracle、`[verify: bash scripts/test_redis.sh]` 锚定验收,并要求记录串行墙钟/token 作为 [`PARALLEL_FRESH_GENERATION.md` §7.1](docs/harness/PARALLEL_FRESH_GENERATION.md)「量痛点」的并行对照。**SWE-bench Verified 小样本**(10–20 题)= 验修复路径与「修复不可并行」,含选题/隔离(只给 `problem_statement`、不泄 `test_patch`)、每题填两占位符的可复制 prompt、官方 `run_evaluation` harness 做权威判定(`FAIL_TO_PASS` 转绿 + `PASS_TO_PASS` 保持)、不作弊(不动测试目录)/不并行判定矩阵。均从 [`LHT_TEST_SUITE.md`](docs/harness/LHT_TEST_SUITE.md) §6 回链。
- **测试集补「非确定性」§5.1（DeepSeek V4 思考模式 quirk）：** [`docs/harness/LHT_TEST_SUITE.md`](docs/harness/LHT_TEST_SUITE.md) 新增 §5.1，说明同一 prompt 输出每次不同的三层成因（采样随机 / 系统级浮点·batching·MoE 路由 / agent 级联放大），并按[官方文档](https://api-docs.deepseek.com/zh-cn/guides/thinking_mode)记录 **DeepSeek V4 思考模式静默忽略 `temperature`/`top_p`/`presence_penalty`/`frequency_penalty`**(设置不报错但不生效;思考强度改由 `reasoning_effort` 控制;runtime 也未下发 `seed`)——"调低 temperature 稳复现"这条路封死。推论:测试判定只能靠客观 oracle 判终态行为、不能靠输出比对,坐实「事实源 > 模型声明」。
- **DEMO3 复现规格（可直接喂 runtime）：** 新增 [`docs/harness/test-cases/DEMO3-monkey-interpreter.md`](docs/harness/test-cases/DEMO3-monkey-interpreter.md) —— 把 DEMO3「验收塌缩成创建项」假绿编成完整可复现规格：逐字 prompt（显式点名标准 Monkey 没有、易漏的 `%` 取模与带数字标识符 `counter1` 两个钓鱼特性）、期望的带 `[verify:]` checklist 分解（验收锚点 `[verify: bash scripts/run_examples.sh]`，禁止塌缩成「创建示例脚本」）、客观 oracle + `conformance.sh`（modulo / ident-digits 最小判定脚本兜底）、离线回放命令（grep `[lht-probe] verify_gate` 看 verdict 是否退化成 `untagged_ok`/`mismatch`）与通过/失败判定矩阵。从 [`LHT_TEST_SUITE.md`](docs/harness/LHT_TEST_SUITE.md) §2/§6 回链；新建 `docs/harness/test-cases/` 作为后续案例模板。
- **长程任务测试集（DEMO 压测编纂）：** 新增 [`docs/harness/LHT_TEST_SUITE.md`](docs/harness/LHT_TEST_SUITE.md)（活文档）—— 把仓库真实跑过的 **DEMO2–DEMO5**（2W 行级 Go/Monkey 解释器压测）编为**黄金回归案例**，每条锚定它钓出的静默早停/假绿漏洞（DEMO2 progress-pass 放行、DEMO3 验收塌缩成创建项、DEMO4 step 耗尽、DEMO5 plan/checklist 双计数 + verify_gate 假 mismatch）。外加**外部经典案例映射**（解释器/光追 → 强制续写；SQLite-clone/库级生成 → Cycle+交接；CodeCrafters/全栈 CRUD → 并行 fan-out 闸门；SWE-bench Verified → 修复不可并行；LiveCodeBench → 防污染生成）、**`[verify:]` 编写规范**（「创建文件 ≠ 验证通过」三铁律）、**观测/判定准则**（`[lht-probe]`/`[stream-probe]`/Nodes Tab）与**最小回归集**（Monkey 解释器 + CodeCrafters Redis + SWE-bench 小样本）。选型第一原则 = 自带判定式 oracle，契合「事实源 > 模型声明」。与 [`LONG_HORIZON_CODE_TASKS.md`](docs/harness/LONG_HORIZON_CODE_TASKS.md)、[`PARALLEL_FRESH_GENERATION.md`](docs/harness/PARALLEL_FRESH_GENERATION.md)、`harness/README.md` 互链。
- **全新项目并行代码生成方案（设计对话整理）：** 新增 [`docs/harness/PARALLEL_FRESH_GENERATION.md`](docs/harness/PARALLEL_FRESH_GENERATION.md)（⬜ 规划中，0.8 之后）—— 全新项目/模块生成主模型串行慢，可走**契约优先 fan-out/join** 并行化；**重构/修复不可并行**（已存在代码语义耦合，文件锁拦不住）。核心是两道闸门夹住并行段：**P0.5 契约固化闸门**（分发前，「build the *right* thing」，程序化优先 = 骨架可编译 / 依赖图无环且=租约边界 / 项↔owner 唯一，+ 2–3 轮认知审 → 收敛冻结或降级回串行）+ **P1.5 符合性审核闸门**（集成前、逐模块对冻结契约审，「build the thing *right*?」，接口指纹 diff / 对契约编译 / 跑契约自带测试 → pass 才放行，失败 scoped-fix 或回退契约修订）。复用现有子代理基础设施（`subagent/` 的 spawn/wait/黑板/`resident.rs` 文件租约/深度上限）。本方案是 [`agent-reliability-craft-plan.md` §11.5](docs/agent-reliability-craft-plan.md) 两个金矿（设计评审前置 / 可追溯矩阵）在并行生成抓手上的落地；与 [`harness/LONG_HORIZON_CODE_TASKS.md`](docs/harness/LONG_HORIZON_CODE_TASKS.md)、`harness/README.md` 互链。**文档诚实区分「方法成熟」与「收益未验证」**：§6.1 列理论锚点（依赖分析 / API-first / 设计契约 / V&V / Amdahl / S3>S1），§7.1 摊开 4 条待验证前提（痛点是否真在速度、Amdahl 串行开销是否吃掉收益、单模型并发是否被限流、模型能否一次冻结契约）+ 去风险实验，并把「先量痛点 + 手动模拟一次 fan-out」设为落地前**硬前置闸门**——结论为负则不进开发。定位为待验证方向，非已拍板路线。
- **抗幻觉工程哲学（设计对话整理）：** [`docs/desktop/DEV_NOTES.md`](docs/desktop/DEV_NOTES.md) 新增 `2026-05-30` 章节，凝练 Zagens 两个底层决策的设计哲学——(1) 单模型（V4）路线取舍签收（「单模型深度适配 > 多模型浅适配」，但把模型耦合点收进适配层、勿绑死版本）；(2) 抗幻觉四件事骨架（工具够好不给逃逸动机 / 输入逼 grounding / 输出逼证据 / 终审交给不会幻觉的工具）；(3) CRAFT 出处 = 人类工程项目实施流程，附「人类工程机制 ↔ harness」映射表 + 两个未挖掘金矿（设计评审前置、可追溯矩阵）；(4) 类比边界（模型缺责任心/主动求助/长期记忆 → 靠责任心兜底处须换硬约束；单模型自审共谋盲区 → S3 程序化校验优先级高于 S1 双模型）。带 ✅🔶⬜ 落地状态标注。两个「金矿」（设计评审前置、可追溯矩阵）落成可检索 backlog 锚点：[`agent-reliability-craft-plan.md` §11.5](docs/agent-reliability-craft-plan.md) + [`LONG_HORIZON_CODE_TASKS.md`](docs/harness/LONG_HORIZON_CODE_TASKS.md) 顶部「0.8 之后」交叉引用，三处互链。
- **`write_file` 优化方案:** [`WRITE_FILE_IMPROVEMENTS.md`](docs/tech/WRITE_FILE_IMPROVEMENTS.md) — 行尾保留 / 编码安全 / 原子写 / JSX 与截断信号 / 大量代码写入（摘要式 diff、字节上限）的优先级方案与验证清单。
- **CRAFT naming SSOT:** [`agent-reliability-craft-plan.md`](docs/agent-reliability-craft-plan.md) — 文首「命名备忘」固定 CRAFT = Closed-loop / Review / Agent / Fix-loop / Traceable（对照路线图 B1.1–B1.4）；[`RUNTIME_EVOLUTION_ROADMAP.md`](docs/tech/RUNTIME_EVOLUTION_ROADMAP.md) §9.1 交叉引用。
- **Harness / LHT:** [`LONG_HORIZON_CODE_TASKS.md`](docs/harness/LONG_HORIZON_CODE_TASKS.md) — §15 Phase 1 Playbook（9 步实施顺序）；§3.3.2 UI 落点（`LongHorizonPanel` 左下格）。

### Desktop (Zagens)

- **Feature (LHT 配置面板 + 首启默认):** 设置 → **LHT 配置** 新面板（`LhtSettingsPanel.tsx`）通过 `get_lht_settings` / `save_lht_settings` 读写 `config.toml` 的 `[long_horizon]` 与 `[long_horizon.completion_gate]`（保留已有 `[[verify]]` / `[[deliverable]]` 自定义条目）；保存后重启 sidecar。首启 `config.toml` 现写入产品默认：`enabled=true`、`mode=auto`、通用三门 `observe`。`zagens-config` 新增 `lht_config.rs` 与 `ConfigToml.long_horizon` 字段。四语 i18n。
- **Fix (启动引导 — 每次启动连接进度 + 缺项才展示配置步):** `OnboardingOverlay` 改为**每次 Tauri 启动**都弹出连接进度条（sidecar `connected` + API Key 状态就绪 + 最短 ~2.2s 后自动收起或进入后续步）；**API Key 页**仅在 `get_api_key_status` 为 `false` 时出现（已配置 keyring/env 则跳过）；**默认模式页**仅在 `localStorage` 尚无 `zagens-desktop-task-type` 时出现（选完或点「开始使用」写入）。移除首启一次性 `zagens-desktop-onboarded` 门闩；任务类型不再在 App 挂载时静默写入 localStorage（避免新装用户永远跳过模式步）。Files: `web-ui/src/components/OnboardingOverlay.tsx`、`web-ui/src/App.tsx`、`web-ui/src/lib/appPreferences.ts`。
- **Feature (首启三步引导 — 解决「装了用不起来=差评」):**** 全新安装/未配置 key 的桌面用户启动后弹出**引导浮层**(`OnboardingOverlay.tsx`),三步走:**① 连接检测进度条** —— 复用 `useRuntimeConnection` 的 `runtimeConn`,`connected` 后进度条转绿并自动进入下一步(`checking`/`offline`/`auth_mismatch` 显示对应状态与自动重试提示);**② DeepSeek API Key** —— 复用 `save_deepseek_api_key`,附「前往获取 API Key」外链(`platform.deepseek.com/api_keys`),可「稍后再填」跳过不硬卡;**③ 默认模式三选一** —— 自动 / 代码 / 办公,写入既有 `zagens-desktop-task-type` 偏好(`DesktopTaskTypePreference`)。完成即写 `zagens-desktop-onboarded` 标记。**仅对新装且无 key 用户显示**:已有 key 的老用户(升级)首次见到 `configured=true` 时静默标记为已引导、永不打扰;非 Tauri(web)环境不触发。新增 `onboarding.*` i18n(en/zh-Hans/ja/pt-BR 四语)。`tsc -b` + `vite build` 通过。Files: `web-ui/src/components/OnboardingOverlay.tsx`(新)、`web-ui/src/App.tsx`(决策一次性状态机 + 挂载)、`web-ui/src/lib/appPreferences.ts`(`isOnboarded`/`markOnboarded`)、`web-ui/src/i18n/locales/{en,zh-Hans,ja,pt-BR}.ts`。
- **Feature (发布打包 — 安装器 zip 封装 + SHA-256 校验,降低未签名 SmartScreen 摩擦):** 未签名安装器经浏览器下载后会被打上 MOTW(网络来源标记),首启弹 SmartScreen 蓝框劝退。新增构建后脚本 `crates/desktop/scripts/package-release.mjs`(`npm run package:release`):把 `target/release/bundle/{nsis,msi}` 下每个安装器(`*-setup.exe` / `*.msi`)连同一份**中英 `README.txt`(含该文件名、SHA-256、解锁/安装/校验步骤、WebView2 说明)**一起封进 `<name>.zip`,并为 zip 与原安装器各生成 `<name>.sha256`(`sha256sum` 兼容格式)。README 在临时目录生成、打完即删,bundle 保持干净。**zip 的价值**:用户在 zip 层「右键→属性→解除锁定」一次,解压出的安装器即不带 MOTW、安装全程不弹 SmartScreen,且 zip 内是**完整安装器**(WebView2 仍会自动装,不同于绿色便携包)。Windows 用内置 `Compress-Archive`(零依赖),非 Windows 回退 `zip`。`release.yml` 新增「Package installers」步骤并把 `.sha256` 纳入 staging;Release 说明改为推荐下载 `*-setup.exe.zip` + 解锁指引。新增落地页可直接复用的中英文档 `docs/desktop/SMARTSCREEN.md`(解锁步骤 / 校验方法 / WebView2 说明 / 为何暂不签名)。注:安装器已自压缩,zip 体积≈原文件(目的是解锁体验与规避浏览器 `.exe` 拦截,非压缩)。Files: `crates/desktop/scripts/package-release.mjs`(新)、`crates/desktop/package.json`、`.github/workflows/release.yml`、`docs/desktop/SMARTSCREEN.md`(新)。
- **Fix (首启依赖 — WebView2 缺失时自动拉取,避免首启崩):** `tauri.conf.json` 显式设置 `bundle.windows.webviewInstallMode = { type: "downloadBootstrapper", silent: true }`。此前未显式配置(虽走 Tauri 默认 downloadBootstrapper,但未声明)。现明确:WebView2 运行时缺失时安装阶段静默自动拉取 bootstrapper 安装,保证首次启动不会因缺 WebView2 而白屏/崩溃。sidecar 运行时(`zagens-runtime`)经 `bundle.externalBin` 早已随安装包自带,无需联网。**注:** 面向纯离线/内网分发可改 `offlineInstaller`(内嵌完整 WebView2 运行时,+~150MB 安装包)。File: `crates/desktop/tauri.conf.json`。
- **Fix (工具流展开后滚动卡顿 — 离屏工具卡跳过渲染/布局):** 助手气泡里折叠的「工具流」展开时，`MessageBubble.tsx` 会用 `message.tools.map(renderToolCard)` **一次性挂载全部工具卡**；工具调用一多（几十张卡，每张 `ToolCard` 的 input/output 还是带 `overflow-auto` 的 `<pre>` 嵌套滚动盒、内容可能很大），展开瞬间就要同步布局/绘制所有卡 → 展开一刹那资源飙升，之后拖动滚动条每帧都要重排这些**离屏**嵌套滚动容器，整体明显变慢（用户实测反馈）。修复：给每张工具卡套一层 `.tool-stream-item` 包裹，用 CSS **`content-visibility: auto` + `contain-intrinsic-size: auto 96px`**——浏览器对视口外的卡片**跳过渲染与布局**（WebView2/Chromium 原生支持），滚动到附近才渲染，intrinsic-size 占位保证滚动条稳定不跳动；展开瞬间只渲染视口附近的卡，滚动也只重排可见卡。零依赖、零行为变化（复制/find-in-page/聚焦滚入时浏览器自动补渲染）。Files: `web-ui/src/components/MessageBubble.tsx`、`web-ui/src/styles/globals.css`。
- **LHT panel (plan display-only 大纲淡化 + 新 Node 颜色 — DEMO3 #A / 配合软门禁):** 当 checklist 为完成权威（非空）且任务 **100%** 时，plan 里仍 `pending` 的阶段属**展示用大纲**而非未完成工作（DEMO5 #1：checklist 单独驱动 `completion_pct`）。`LongHorizonPanel.tsx` 现在在 Plan 标题旁加注记「大纲（以清单为完成依据）」（新增 i18n key `longHorizon.planOutlineNote`，en/zh-Hans/ja/pt-BR 四语），并把这些 pending 阶段**淡化 + 删除线**，消解「进度条 100% 但清单看着没关闭」的认知错位（用户实测反馈）。Nodes Tab 新增 `unverified_acceptance_nudge` 节点配色（橙色 `orange-600/400`，与 verify mismatch 同色系，标识 DEMO3 假绿门禁触发）。Files: `web-ui/src/components/LongHorizonPanel.tsx`、`web-ui/src/i18n/locales/{en,zh-Hans,ja,pt-BR}.ts`。
- **LHT panel (task timer + status colors):** the 长程任务 task-graph panel header now shows a number-only **elapsed-time stopwatch** in its top-right (`LongHorizonPanel.tsx`): a client-side timer that starts when a task graph first appears, ticks `mm:ss` (rolls to `h:mm:ss` past an hour) while the task is incomplete, and **freezes green on 100% completion**. It now also **restarts from zero when the next round begins** — i.e. when completion drops back below 100% after a frozen finish (new prompt re-opens the checklist/plan), instead of resuming from the stale start timestamp and jumping forward (resets on thread switch — no backend timestamp). Checklist/plan **status colors** are now semantic: completed items render **green** (dot + text, `emerald`), in-progress **sky-blue + bold**, pending muted — replacing the prior flat muted/emphasized two-tone, so progress reads at a glance. The completion **progress bar** (`█░`) now tints its filled segment **amber** (`amber-600` light / `amber-400` dark, both legible) against a muted track, instead of the prior flat-muted bar where fill was invisible; light + dark mode both handled.
- **Fix (empty-body truncation root cause — default `max_tokens` too low for thinking mode):** desktop default output budget raised **8192 → 393216 (384K, DeepSeek V4 official max output)** (`web-ui/src/lib/modelParams.ts`, `MODEL_MAX_TOKENS`/`DEFAULT_MAX_TOKENS`), and the model-params dialog cap raised **65536 → 384K** (`ModelParamsDialog.tsx`, now `step=1024` + apply-time clamp to `[256, 384K]`). V4 models default to **thinking mode**, where `max_tokens` budgets `reasoning_content` **and** the answer together; on a reasoning-heavy coding turn the model burned the entire 8192 on chain-of-thought and the provider closed with `finish_reason=length` before any answer/tool call — the silent "empty body + `Completed`" truncation. Confirmed from `sidecar.log`: `stop_reason=Some("length") out_tokens=8191 max_tokens=8192 text_bytes=0 tool_uses=0 thinking_bytes=28971` (the earlier monitor backpressure fix removed a *separate* stall — `rx_backlog` stayed single-digit here; this cap is the actual truncation). `max_tokens` is an upper bound, not a reservation, and the engine forwards the UI value to the API unchanged (`streaming_phase.rs`: no clamp; `effective_max_output_tokens`/`TURN_MAX_OUTPUT_TOKENS` only apply when the UI sends nothing / to input budgeting). `loadModelParams` **migrates** any stored `maxTokens ≤ 65536` (the old 8192 default + interim 65536) up to the new default so existing installs stop truncating without manually re-opening the dialog.
- **Fix (composer lock desync):** The chat input no longer unlocks while the backend turn is still running. The desktop SSE proxy (`pollThreadTurnEventsViaTauriProxy`) treated *any* stream close as turn-end, so a long/quiet turn (slow build, blocked exec, model thinking) that dropped the connection before `turn_completed`/`done` left the composer enabled — and the next send hit `Thread already has an active turn`. It now tracks the last seq, and on a non-terminal close reconciles with the backend (`threadTurnStillActive` via `GET /v1/threads/{id}` turn status) and reconnects from the latest seq while the turn is active, keeping the composer locked until the turn truly ends. As a safety net, a rejected send that hits an "active turn" error now rolls back the optimistic bubbles and reconnects to the live turn (`composer.turnStillRunning` toast) instead of surfacing a raw error.
- **LHT panel:** Audit grid lower-left **LongHorizonPanel** — tabs 任务图 / Cycle / 上下文; polls `GET …/harness/task-graph`, `…/harness/cycles`, and thread context; SSE `harness.task_graph`; `useHarnessGridData` extends grid visibility.
- **LHT chip:** Composer footer shows `long_horizon.continue_injected` / `blocked` / `context_warning` from SSE status.
- **LHT cycles:** `LongHorizonPanel` refreshes task graph + cycle tab on `harness.cycle_advanced` SSE.
- **LHT context UI:** Context tab shows usage bar with cycle threshold marker and LHT warning band; sidecar-ready panel recovery for harness grid + long-horizon panel.
- **Fix (LHT panel Context tab not live during streaming):** the long-horizon panel's **Context** tab only refreshed on the 30s task-graph poll (`TASK_GRAPH_POLL_STREAMING_MS`) while a turn was streaming, so token/usage stats lagged badly behind the live turn. The real-time `panel.context` SSE (channel C) was already dispatched as `PANEL_CONTEXT_EVENT` and consumed by the composer footer (`useThreadContext`), but **no one subscribed to it for the panel** — the push was dropped. `LongHorizonPanel` now listens to `PANEL_CONTEXT_EVENT` and applies the snapshot directly (`setContext`), so the Context tab updates live in lockstep with the composer footer instead of waiting up to 30s.
- **Session restore:** `replayThreadEvents` no longer aborts after 750 ms idle when rebuilding chat history from thread events — waits for SSE stream close instead, fixing truncated messages on long or tool-heavy sessions.
- **i18n → model language:** Settings → Language now syncs to `~/.zagens/settings.toml` `locale`, which drives the runtime system prompt `## Environment` `lang` field — model replies follow the selected UI language (start a **new chat** after switching; existing threads keep their spawn-time locale).
- **Agent panel:** Polls `subagents.v1.json` during streaming (3s) for live step count, per-step timeout cap, and stuck-suspected hint when idle exceeds step timeout + 60s.
- **Page reload guard:** Block F5 / Ctrl+R (Cmd+R) full-page refresh — prevents accidental loss of in-memory chat state before persist-session completes; shows a brief toast instead.
- **Audit grid panel:** When checklist, audit scratchpad, or sub-agent data appears, a 2×2 right-side grid (checklist / audit / reserved / sub-agents) auto-opens and temporarily replaces the single Inspector panel; auto-hides when all three are empty; title-bar grid toggle and seam collapse respect manual dismiss until data clears or the thread changes.
- **Symbol index panel:** Freshness check covers JS/Python/Go/C++/Vue sources and flags stale indexes below schema v5.
- **Binary preview:** Cap reads at 10MB without loading entire files into memory (`read_binary_file_at`).
- **Shell open:** `open_in_shell` canonicalizes paths and rejects shell metacharacters (aligns with `open_with_system_app`).
- **KV cache observability:** Usage dashboard shows hit rate %, miss tokens, and estimated cache savings (USD); composer footer shows last-turn `cache XX%` with red/yellow thresholds; warns when provider lacks cache telemetry. See [`docs/tech/KV_CACHE_OBSERVABILITY.md`](docs/tech/KV_CACHE_OBSERVABILITY.md).
- **Build (Windows MSI):** Set `bundle.windows.wix.version` to numeric `0.6.0.1` so WiX accepts pre-release SemVer `0.6.0-preview.1`; document mapping in [`VERSIONING.md`](docs/desktop/VERSIONING.md) and CI check.

### Runtime

- **Prompt language enforcement:** `## Environment` now includes `reply_language` plus a mandatory reply-language line; `base.md` explicitly maps `lang: en` → English and states Chinese audit examples do not override locale — reduces first-turn Chinese replies when UI locale is English.
- **Symbol index (V6):** Extend lazy workspace index to JavaScript (`.js`/`.jsx`/`.mjs`/`.cjs`), Python (`.py`), and Go (`.go`); bump `schema_version` to 4; `grep_files(symbol_index: true)` now returns `calls` when present; split parsers into `symbol_index/extract.rs`. See [`docs/symbol-index-v6-improvements.md`](docs/symbol-index-v6-improvements.md).
- **Symbol index (V7):** Add C/C++, Vue/Svelte SFC parsers; CamelCase fuzzy + caller reverse lookup; parallel build via `std::thread::scope`; sidecar startup warmup; `schema_version` 5. See [`docs/symbol-index-v7-improvements.md`](docs/symbol-index-v7-improvements.md).
- **Fix (symbol index / sidecar):** Sidecar no longer defaults workspace to `%USERPROFILE%` — passes `--workspace` (`Documents/Zagens`) and skips V7 warmup on user-home / non-project roots; expands index walk skip dirs; caps `annotate_calls` regex size (fixes ~8GB RAM + `/health` connection refused on startup).
- **Audit follow-ups (2026-05-28 report):** Binary preview reads at most 10MB via streaming I/O; SQLite RFC3339 parse errors surface instead of silent epoch; execpolicy prefix allow rules no longer match chained shell commands; runtime API bearer compare is constant-time.
- **Sandbox:** When `sandbox_backend` is configured but initialization fails (or the value is invalid), emit a user-visible status warning instead of silently falling back to unsandboxed shell execution.
- **Cleanup:** Remove unused `crates/execpolicy` (`deepseek-execpolicy` / `ExecPolicyEngine`); runtime uses `runtime-server/src/execpolicy/` + `command_safety.rs` instead.
- **Audit scratchpad deadlocks:** `scratchpad_import_agent` honors `area_id` override and remaps by `area_path` when child ids mismatch inventory; `scratchpad_set_area(deferred)` defaults `require_min_notes=0` (meta note still required).
- **Audit scratchpad (Turn-1 report gap):** `scratchpad_import_agent(block=true)` now enriches via `get_result_with_fallback` after wait (fixes missing structured import); truncated `<!-- audit-findings -->` JSON salvages complete items; `checklist_write`/`checklist_update` warn on checklist↔inventory mismatch; active scratchpad blocks prose-only turn break when P2 gates unmet; audit report `write_file` gate covers `doc/*audit*` and `CODE_AUDIT*.md`; `scratchpad_status` returns `contract_hints`.
- **Audit scratchpad (P2 quality gate UX):** `write_file` block reason and `scratchpad_status` now list `areas_failing_quality_gate` (e.g. `done` with meta-only notes); contract hint no longer says “inventory closed” when `accounted_ratio` is below the 60% hard threshold.
- **Sub-agent progress & stuck detection (P2):** Executor updates `steps_taken` / `progress_status` in manager on each step heartbeat; `agent_list` / disk snapshot expose `stuck_suspected` and `idle_ms`; 30s background zombie scan (P2-10); AgentPanel polls `subagents.v1.json` during streaming with step/cap/stuck UI; audit-repo skill adds outlier cancel+defer rule.
- **Skills (`audit-repo`):** Require `checklist_write` after `scratchpad_init` so sidebar 清单 mirrors inventory areas; clarify inventory vs Checklist panel dual-track; document partial-audit defer + import override playbook.
- **Audit scratchpad + sub-agents:** Sync `scratchpad_run_id` from the tool wire slot at turn start and after successful scratchpad bind tools; mid-turn `scratchpad_init` now eager-loads `agent_spawn` / `agent_*` in the same turn without `tool_search`.
- **`GET /v1/usage`:** `UsageTotals` / `UsageBucket` now include `miss_tokens`, `cache_hit_rate`, `cost_usd_without_cache`, `cache_savings_usd`; response adds `cache_telemetry_incomplete` when any turn used a model without DeepSeek-style cache fields.

### Process

- **Versioning:** Zagens 预发布渠道采用 SemVer 预发布标识（默认 **`0.x.y-preview.n`**）；SSOT [`docs/desktop/VERSIONING.md`](docs/desktop/VERSIONING.md)；四处 manifest 对齐 **`0.6.0-preview.1`**。
- **Docs:** Add [`docs/tech/SUBAGENT_STABILITY_ANALYSIS.md`](docs/tech/SUBAGENT_STABILITY_ANALYSIS.md) — sub-agent spawn/execute/join timeout layers, terminal-state gaps (panic zombie Running, step-limit false Completed), structured persistence, and P0–P2 remediation priorities for audit-repo boundary stability.
- **Docs:** Update [`SUBAGENT_STABILITY_ANALYSIS.md`](docs/tech/SUBAGENT_STABILITY_ANALYSIS.md) implementation status — P0/P1 marked landed (`dfe9eb1`), §9–§11 acceptance checklists checked, P2 deferred.

### Runtime

- **Sub-agent stability (P0):** Panic in child tasks now marks agents **`Failed`** (with crash dump) instead of leaving zombie **`Running`**. **`completion_reason`** on `SubAgentResult` / `<deepseek:subagent.done>` distinguishes **`NaturalBreak`** vs **`StepLimitReached`**, cancel, step API timeout, and panic. **`agent_wait` / `agent_result(block)`** use **adaptive join timeout** when `timeout_ms` is omitted (`step_timeout_ms × remaining steps`, clamped 10s–1h); `timed_out` responses include progress fields. Updated `audit-repo` skill and `base.md` wait guidance.
- **Sub-agent stability (P1):** `resume` clears stale `structured_findings` / `structured_verdict`; `agent_send_input` rejects finished task handles with an explicit error; parent turn **`no_tool_uses`** re-drains the completion channel when `running_count` is zero (closes completion race).
- **Sub-agent stability (P1 cont.):** Per-step shared tool timeout budget (80% of `step_timeout`); `ParseFailureReason` when `<!-- audit-findings -->` parse fails; persist `completion_reason` / `blackboard_task_id` (schema v1, backward compatible); `agent_result` / `scratchpad_import_agent` use **`get_result_with_fallback`** (memory → blackboard → prose); **`scratchpad_import_agent`** rejects agents whose `completion_reason` is `StepLimitReached`, `StepApiTimeout`, or `Cancelled`.
- **Sub-agent stability (P1 multi-run):** `agent_spawn` binds **`scratchpad_run_id`** from active scratchpad context; **`scratchpad_import_agent`** rejects run mismatch and `area_id` not in target run inventory (prevents cross-run note pollution).
- **Audit / sub-agents:** Structured `<!-- audit-findings -->` output on Explore/Review sub-agents (`StructuredFindings` on `SubAgentResult`); new tools `scratchpad_import_agent` (machine import as `open`) and `scratchpad_verify_note` (parent verification gate); `scratchpad_set_area(done)` rejects open HIGH/BLOCKER; new findings default to `status=open`; `scratchpad_init({ template: "workspace_audit" })` auto-builds inventory from workspace `Cargo.toml` members (includes `runtime-server` + desktop web-ui areas). Updated bundled `audit-repo` skill.
- **Sub-agents / config:** Raise default per-step LLM API timeout to **600 s** (was 120 s); clamp range **120–1800 s** (was 10–600). `agent_spawn` `step_timeout_ms` schema and runtime clamp aligned; audit-repo skill documents inventory file-count tiers (600000–1800000 ms). Zagens settings slider updated.
- **Audit scratchpad / isolation:** `GET …/scratchpad/status` no longer discovers the newest workspace run or auto-writes `scratchpad_run_id` on unrelated threads. Only threads that init or use scratchpad tools in-session show audit progress; on-disk runs persist until manual delete.
- **Audit scratchpad / multi-run:** Threads track `scratchpad_run_history`; status API returns latest run at top level plus `previous_runs` (folded in Zagens audit panel). Agent `scratchpad_init` with a new `run_id` auto-promotes the latest audit — no manual switch.

### Fixed

- **Security (2026-05-28 audit HIGH):** Block project `.deepseek/config.toml` from overriding global `allow_shell` / `approval_policy` / `sandbox_mode`; replace Windows `cmd /C start` in `open_with_system_app` with canonical path validation + `open` crate (ShellExecute); re-sanitize ChatMarkdown after workspace-link DOM enhancement (mXSS); DOMPurify-sanitize highlight.js output in CodeRenderer.

### Desktop

- **Fix:** 工作台「恢复」面板 — 修复 side-git 快照仓库并发 `git init` / 残留 `config.lock` 导致 `HTTP 400`；Web UI 解析 runtime JSON 错误体，不再整段显示原始 HTTP 响应。
- **Perf:** 冷启动 — Web UI 不再阻塞 `get_runtime_port` 才首渲染；React 挂载后立即 `show()` 主窗口；sidecar 在 Tauri `setup` 中提前 spawn（与 WebView 加载并行）。会话列表仍由 `useRuntimeConnection` 在 runtime 就绪后加载。
- **OTA（预埋）：** Tauri updater endpoint 指向 `https://zagens.com/download/latest.json`（官网未上线；`pubkey` / `tauri-plugin-updater` / CI 签名待后续接入）；CSP `connect-src` 放行 `zagens.com`。
- **i18n:** 桌面 Web UI 按系统语言自动选择界面语言（`navigator.languages` 优先）；无对应语言包时默认 **English**；用户曾在设置中手动选择的语言仍优先（localStorage）。
- **Fix:** 多窗口 — 活跃会话 `localStorage` 按窗口 label 隔离（`zagens-desktop-active-session-id:{label}`）；新建第二窗口不再自动恢复第一窗口的会话；主窗口一次性迁移旧全局键。
- **Fix:** 侧栏设置折叠菜单 — 接入 i18n（设置、MCP 服务器、模型路由、索引、系统设置、Sessions 等硬编码中文/英文）。
- **Fix:** MCP 服务器面板 — 接入 i18n（状态栏、添加/合并按钮、空状态、对话框与子表单等）。
- **Fix:** 模型路由面板 — 路由策略选项与 Composer 路由状态 chip 接入 i18n。
- **Fix:** 空会话欢迎页与 Composer 任务类型/运行模式说明 — 接入 i18n。
- **Fix:** 错误与提示文案 — 消息气泡、终端卡片、用量面板、任务面板、附件/视觉桥接错误、聊天渲染失败、完成通知等接入 i18n。
- **Fix:** 补齐 `~/.zagens/` 迁移遗漏 — `automations`、`audit.log`、`topic-memory`、`office-py`、`execpolicy.toml`、`tui.toml`、skills cache、crash dumps 等用户级路径不再写入 `~/.deepseek/`（工作区 `.deepseek/` 仍保留 scratchpad/blackboard/项目 config）。
- **Fix:** `prepare-python.mjs` — 校验 PBS 压缩包完整大小（对比 GitHub `Content-Length`），自动删除中断留下的残缺包并重下；下载进度日志；解压失败时清理部分目录避免下次误判。

- **Chore:** `zagens-cli` — `cargo fix` 清理 subagent / web_run / skills 等模块 D16 拆分遗留的 unused import；修正 `TarballScan` / `SubAgentSpawnOptions` 可见性警告。

- **Docs:** [`RUNTIME_ARCHITECTURE.md`](docs/tech/RUNTIME_ARCHITECTURE.md) 全面与 D16/D17 代码对齐 — 四 crate sidecar 栈、Turn 生产路径、持久化落点、依赖图；§1.1 增加中文版系统总览示意。
- **Docs (D17 修订):** [`D17_ARCHITECTURE_FREEZE.md`](docs/tech/adr/D17_ARCHITECTURE_FREEZE.md) 与实现对齐 — Turn 链（orchestrator → `TurnEnginePort` → sidecar `dispatch_op` → `handle_deepseek_turn`）、SubAgent 锚点路径、`I1`/边界测试范围、`I7`/F2 去 ratatui 误述、OpenAPI 护栏（脚本 + `ci.yml`）、持久化默认路径与环境变量覆盖说明。
- **Docs (D17 收尾):** D16 三 crate（`runtime-adapters` / `runtime-orchestrator` / `runtime-api`）`lib.rs` 文档更新 — 移除"待迁移"口吻，明确标注 D17 冻结；further extraction deferred by design。
- **D17 (Landed):** Architecture Freeze v1 — 重构主线关闭；D16 Closed (Checkpoint)；明确 **不执行** E1 阶段 2 / E4 / runtime-server <500 行 KPI / Harness 分离。见 [`docs/tech/adr/D17_ARCHITECTURE_FREEZE.md`](docs/tech/adr/D17_ARCHITECTURE_FREEZE.md)。
- **D15 (Landed):** Final architecture convergence — removed `deepseek-state` crate and legacy `core::Runtime` / `ThreadMessageTurnPort`; Zagens Desktop is the sole user entry; sidecar spawn unified to `deepseek-runtime` only. Session remains a projection of `RuntimeThreadStore` (D7 `runtime_thread_id` link). See [`docs/tech/adr/D15_FINAL_ARCHITECTURE_CONVERGENCE.md`](docs/tech/adr/D15_FINAL_ARCHITECTURE_CONVERGENCE.md).
- **Docs (D16):** Phase E maintainability split plans — [`docs/tech/adr/D16_PHASE_E_MAINTAINABILITY.md`](docs/tech/adr/D16_PHASE_E_MAINTAINABILITY.md) (`runtime-server` crate、SubAgent、`App.tsx` hooks；不阻塞发布).
- **D16 E2 (Landed):** Split `tools/subagent/mod.rs` (~4340 行) into focused modules — `mod.rs` ~82 行、`manager.rs` / `executor.rs` / `tools/*` / `parse.rs` / `router.rs` / `prompts.rs` 等；108 个 subagent 单元测试全绿。
- **D16 E3-a (Landed):** Extract `hooks/useRuntimeConnection.ts` from `App.tsx` (Sidecar boot、probe、重连、runtime 状态).
- **D16 E3-b/c/d (Landed):** `useTurnSession` / `useTurnStream` / `useTurnApproval` / `useAgentPanelState` / `useTurnSend`；`AppShell` + `App.tsx` **776 行**。
- **D16 E1-a3–a6 (Landed):** tools host 端口、workspace_walk/arg_repair、network_gate；fetch_url/web_run/web_search 去重。
- **D16 E1-a7:** `skills/install.rs` 复用 `network_gate::check_host_with_policy` / `host_policy_decision`。
- **D16 E1-a8:** `tools/shell/tools.rs`（~1057 行）再拆为 `shell/tools/{exec,wait,cancel,note,helpers}.rs`；22 个 shell 单元测试全绿。
- **D16 E1-a8 (WIP):** `tools/file.rs`（~1991 行）模块内拆为 `file/{read,write,edit,list_dir}.rs` + `tests.rs`；`sniff_encoding_label` 仍由 `file_info` 复用；30 个 file 单元测试全绿。
- **D16 E1-a8 (WIP):** `tools/web_run.rs`（~1638 行）模块内拆为 `web_run/{types,state,tool,search,page,html}.rs` + `tests.rs`；11 个 web_run 单元测试全绿。
- **D16 E1-c6:** task wire 类型（`TaskRecord`、`TasksResponse`、`NewTaskRequest` 等）迁入 `runtime-api/src/task.rs` 并加入 `SCHEMA_EXPORTS`；sidecar `runtime_api/openapi.rs` 瘦身为 re-export；`task_manager` 删除重复 struct；`check-openapi-contract` + `runtime_api`/`task_manager` 回归全绿。
- **D16 E5 (Landed):** OpenAPI + TS 契约重新对齐 E1-c；CI `generate:api-types` diff gate；`check-openapi-contract.{sh,ps1}`。
- **D16 E1-b (WIP, phase 1):** 新建 `crates/runtime-orchestrator` — 迁入 `runtime_threads/{types,persist}`、`thread_store_sqlite`、`pricing`（usage 聚合）；`RuntimeThreadManager` 等 live orchestration 仍留 `runtime-server`；40 个 `runtime_threads` 单元测试全绿。
- **D16 E1-b (WIP, phase 2):** 迁入 `runtime_threads/{routing,events,event_coalesce}` 至 orchestrator；新增 `engine`/`engine_host`（`EngineHandle<P,R>` + `RuntimeThreadHost` trait）；`active`/`turn_wait`/`turn_control`/`turn_lifecycle` 核心迁入 orchestrator；`RuntimeThreadManager<P,R>` 核心 + `thread_crud` 在 orchestrator；sidecar `Deref` 包装实现 host（`engine_load`/`monitor`/`prepare_start_turn_params`）；server 保留 Config/task/scratchpad 与 symbol index hook。
- **D16 E1-b (WIP, phase 3):** `monitor_turn` 事件循环（~930 行）迁入 `runtime-orchestrator`/`monitor.rs`；新增 `RuntimeThreadMonitorHost`（panel SSE、artifact refs、全权限 sandbox policy）与 `monitor_persist` 阻塞落盘 helper；sidecar `monitor_host.rs` 实现 host hook；删除 `runtime-server`/`monitor.rs`。
- **D16 E1-b (WIP, phase 4):** `ensure_engine_loaded` 通用路径（缓存、session sync、LRU）迁入 orchestrator/`engine_load.rs`；`RuntimeThreadHost::spawn_engine_for_thread` 由 sidecar `engine_spawn.rs` 实现；`turn_lifecycle`/`turn_control` 直接调用 orchestrator `ensure_engine_loaded(mgr, host, …)`。
- **D16 E1-b (WIP, phase 5):** 新增 `RuntimeThreadTaskPort`（background task 最小 turn 面）与 `RuntimeThreadBackgroundSlots`（task/automation 注入 `RuntimeToolServices`）；`EngineTaskExecutor` 改走 task port。
- **D16 E1-c (WIP, phase 1):** 新建 `crates/runtime-api`（`zagens-runtime-api`）；OpenAPI `paths`/核心 `schemas` 迁入；sidecar `runtime_api/openapi.rs` 合并 task  schema；`export-runtime-openapi` 行为不变。
- **D16 E1-c (WIP, phase 2):** `auth`/`health`/`cors`/`compose_router` 迁入 runtime-api；`RuntimeApiAuthState`/`RuntimeApiProbeState` host trait；sidecar `router.rs` 仅保留 `/v1/*` handler 接线；`/v1/*` bearer 中间件仍在 sidecar 挂载以满足 Axum 0.8 状态类型。
- **D16 E1-c (WIP, phase 3):** `ApiError` 与 `IntoResponse` 错误 envelope 迁入 runtime-api；handler 仍留 sidecar。
- **D16 E1-c (WIP, phase 4):** 共享 wire response（`SessionsListResponse`、`SessionDetailResponse`、`ResumeSessionResponse`、`StartTurnResponse`、`ThreadSummary`）由 runtime-api 导出；sidecar handler 删除重复 struct。
- **D16 E1-c (WIP, phase 5):** `StreamTurnRequest` 由 runtime-api 导出；`stream.rs` handler 复用 wire 类型（`workspace: Option<String>` → `PathBuf` 在 handler 内转换）。
- **D16 E1-d2:** `zagens_runtime::run_http_server` crate 根 re-export；`RUNTIME_ARCHITECTURE` / D8 对齐 runtime-api OpenAPI SSOT 与 D16 crate 依赖图。
- **D16 E1-b (WIP, phase 6):** `task_manager.rs`（~1500 行）模块内拆为 `task_manager/{config,executor,manager,persist,helpers,tests}.rs`；wire 类型仍用 runtime-api；4 个 task_manager 单元测试全绿。
- **D16 E1-a8:** `skills/install.rs`（~1534 行）模块内拆为 `install/{types,api,local,registry,download,tests}.rs`；`pub mod skills` 供集成测试；16 单元测试 + `skill_install` 集成测试全绿。

### Changed

- **Zagens web-ui:** add `lib/workspacePaths.ts` — `joinWorkspaceSegments` helper for native-style workspace path joins (display / Tauri `open_in_shell`).
- **Docs / Harness 文档集：** 新建 [`docs/harness/`](docs/harness/README.md) — 迁入 [`Agent+Harness组合式编程方案.md`](docs/harness/Agent+Harness组合式编程方案.md)、[`HARNESS_INTEGRATION_PROPOSAL.md`](docs/harness/HARNESS_INTEGRATION_PROPOSAL.md)；新增 [`ANTHROPIC_MANAGED_AGENTS_AND_HARNESS.md`](docs/harness/ANTHROPIC_MANAGED_AGENTS_AND_HARNESS.md)（Managed Agents 时间线、官方 Engineering 文章、三模式与组合式方案对照）；`docs/tech/adr/HARNESS_INTEGRATION_PROPOSAL.md` 保留重定向 stub。
- **Docs / Harness v1.3：** [`Agent+Harness组合式编程方案.md`](docs/harness/Agent+Harness组合式编程方案.md) 增补 **阶段六「自适应主动 Harness」**（§3.4 定义、Manifest 一等公民、§10 路线图阶段六）；[`README.md`](docs/harness/README.md) 演进假设表；归并提案 §3 映射「自适应主动」行。
- **Docs：** [`docs/prompt-architecture.md`](docs/prompt-architecture.md) 对齐 D6（`crates/runtime-server` 路径、`task overlay`、Engine 模块拆分）。
- **Zagens desktop / 图标资产：** 新增 `crates/desktop/icons/svg/` — 5 种 SVG 变体及 `preview.html`；神经网络另含 `variants/` 下 6 种配色 + `preview-palettes.html` 对比页（基准：暖白 + 琥珀）。

### Fixed

- **Zagens desktop / 多窗口：** 修复打开第二个窗口时控制台 `Cannot read properties of undefined (reading 'handlerId')` 及 Network 面板 `listen`/`unlisten`/`show`/`runtime_http` 数千次循环 — sidecar/SSE/终端事件改为 webview 级 `listen` + 安全 `unlisten`；启动 effect 不再依赖每轮渲染变化的 `refreshSessions`，并用 `bootHandled` 保证就绪逻辑只执行一次。
- **Zagens desktop / Tauri IPC：** 修复控制台持续刷屏 `[TAURI] Couldn't find callback id …` — 续聊 turn 在桌面端改为单条 SSE（不再每 120ms 重注册 `listen`/`runtime_get_sse`）；`refreshApiKeyStatus` 与终端/ sidecar 事件订阅改为稳定依赖，避免每次渲染重复 `invoke`。
- **Zagens desktop / API Key 面板：** 主模型区新增「删除 API KEY」（二次确认，清除系统密钥链与 config 明文）；精简面板说明文案；接入 i18n。
- **Zagens desktop / 首次配置：** 首次启动（及 runtime sidecar 启动）自动创建 `~/.zagens/config.toml` 默认模板（不含 API Key）；`zagens-config` 新增 `ConfigStore::ensure_default_on_disk` / `ConfigToml::first_run_defaults`；runtime `ensure_config_file_exists` 改为委托 config crate。
- **Zagens desktop / 发布命名：** 主程序二进制 **`zagens.exe`**、sidecar **`zagens-runtime.exe`**（Tauri `externalBin`）；全局用户数据目录 **`~/.zagens/`**；首次启动可选从 legacy `~/.deepseek/config.toml` 迁移配置与 skills/MCP（不迁移 sessions/tasks 数据库）。
- **Runtime / Scratchpad：** 新增 `scratchpad_init` 工具与 `POST /v1/threads/{id}/scratchpad/init` — 自动创建 `{workspace}/.deepseek/scratchpad/{run_id}/`（`inventory.json` + `notes.jsonl`）并绑定 thread；Zagens 审计面板空态支持一键初始化。
- **Zagens desktop / CRAFT：** `GET /v1/blackboards` 支持 `?workspace=`（与 `/v1/workspace/browse` 一致）；AgentPanel 按当前 Composer 工作区拉取黑板，修复 D6 后 sidecar 默认 cwd（用户目录）与子 Agent 写入项目 `.deepseek/blackboards/` 不一致导致 CRAFT 任务列表为空。
- **Runtime：** 移除已删除 `eval.rs` 的孤儿集成测 `eval_harness.rs`（D6 迁移遗留，阻塞 `cargo test -p zagens-cli`）。

## [0.5.0] - 2026-05-26

### Zagens (desktop)

- **v0.5.0** — 架构升级里程碑：`zagens-desktop`、`tauri.conf.json`、`web-ui/package.json` 与 About 面板对齐 **v0.5.0**。主线：D6 Phase B（`deepseek-runtime` 单 crate、移除 CLI/TUI）、M7/M8（Engine 入 core）、D1/D4/D7/D8/D9/D10 与 Assessment **10/10** 定型；含多窗口空白修复与会话侧栏就绪重载。

### Fixed

- **Zagens desktop / 多窗口：** 修复第二个（及后续）窗口空白——Windows 上在同步托盘/命令里创建 `WebviewWindow` 会触发 WebView2 死锁；`create_agent_window` 改为 `async`，托盘与单实例路径改 `spawn`；新建窗与主窗一致先 `visible(false)`，sidecar 已就绪时 `emit_to` 补发 `sidecar://ready`，前端启动门增加就绪探测。
- **Zagens desktop / 侧栏会话列表：** sidecar 就绪前 `GET /v1/sessions` 失败后于 `sidecar://ready` 自动重载；回合结束时在 `finishOnce` 兜底 `persist-session`（修复 SSE 事件异步过滤导致未写入）；工作区路径比较改为大小写/分隔符无关，避免会话被误过滤。

### Changed

- **Architecture / D6 Phase B 文档同步（2026-05-26）：** [`RUNTIME_ARCHITECTURE.md`](docs/tech/RUNTIME_ARCHITECTURE.md)、[`D6_IMPLEMENTATION_PLAN.md`](docs/tech/adr/D6_IMPLEMENTATION_PLAN.md)、[`D6_RUNTIME_SERVER.md`](docs/tech/adr/D6_RUNTIME_SERVER.md)、[`API_DESIGN.md`](docs/tech/API_DESIGN.md)、[`ARCHITECTURE_ASSESSMENT_2026-05-25.md`](docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md)、[`DEV_NOTES.md`](docs/desktop/DEV_NOTES.md)、[`README.md`](README.md) 同步 Phase B 落地态（`deepseek-runtime` 单 crate；路径 `crates/runtime-server`）。
- **Architecture / D6 Phase A+ (2026-05-26):** `deepseek-runtime` binary sidecar contract test — [`sidecar_binary_contract.rs`](crates/runtime-server/tests/sidecar_binary_contract.rs)（spawn 真实 binary + `DS_PICK_READY` + health/thread/SSE/interrupt）；CI ubuntu job 新增 `cargo test -p zagens-cli --test sidecar_binary_contract`；D6 ADR acceptance 第 4 项 ✅。
- **Docs / D6 implementation plan (2026-05-26):** [`D6_IMPLEMENTATION_PLAN.md`](docs/tech/adr/D6_IMPLEMENTATION_PLAN.md) — Phase A 回顾清单、Phase A+ binary CI 契约测、Phase B `runtime_api`/`runtime_threads` 物理迁移 PR 链；配套 [`D6_RUNTIME_SERVER.md`](docs/tech/adr/D6_RUNTIME_SERVER.md)。
- **Docs / RUNTIME_ARCHITECTURE (2026-05-26):** [`RUNTIME_ARCHITECTURE.md`](docs/tech/RUNTIME_ARCHITECTURE.md) 与代码复核对齐 — 生产 sidecar 改为 **`deepseek-runtime`**（D6）；`Engine` + `op_loop` 落点改为 **core**（M-series）；移除已删 `app-server`；§1/§2/§3 依赖图与 §10 架构定型 **10/10** 叙事同步 Assessment。
- **Architecture / D1 (2026-05-26):** `config.rs` → `config/{mod,providers,types,load/}`；`load/` 再拆为 `impl_config`、`paths`、`env_overrides`、`model`、`merge`、`credentials`（实现均 ≤644 行；`tests.inc.rs` ~1.8k 经 `load/mod.rs` include）。脚本 [`split-config-load-rs.py`](scripts/split-config-load-rs.py) 按函数边界切片；`config::load::` **78** 项测试通过。
- **Architecture / D1 (2026-05-26):** `compaction.rs`（~2.7k 行）→ `compaction/{plan,tokens,prune,execute,prompt}.rs` + `tests.inc.rs`（实现 ≤589 行；[`split-compaction-rs.py`](scripts/split-compaction-rs.py)）；`compaction::` **54** 项测试通过。
- **Architecture / D1 (2026-05-26):** `client.rs`（~2.1k 行）→ `client/{tool_names,types,http,client_impl,llm,api_parse,fim}.rs` + 既有 `chat.rs`（[`split-client-rs.py`](scripts/split-client-rs.py)）；`client::` **88** 项测试通过。
- **Architecture / D1 (2026-05-26):** `mcp.rs`（~2.2k 行）→ `mcp/{diagnostics,config,types,transport,connection,pool,config_io,format}.rs`（[`split-mcp-rs.py`](scripts/split-mcp-rs.py)）；`mcp::` **15** 项测试通过。
- **Docs / D1 scope (2026-05-26):** [`ARCHITECTURE_ASSESSMENT_2026-05-25.md`](docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md) — **`tui/ui.rs` 不纳入 D1 拆分**（TUI-only，与 Zagens 桌面无关）；`ui.rs` 模块注释交叉引用。
- **Docs / D1 scope (2026-05-26):** Assessment §5.1 — 扩充「**D1 明确不拆分**」表（ratatui TUI、`localization.rs`、`tools/*`、`legacy.rs`、测试卷等）；§1 #10 收窄为 **`desktop/commands.rs`**（runtime 四模块 `config`/`compaction`/`mcp`/`client` 已拆 ✅）；`localization.rs` 模块注释交叉引用。
- **Architecture / D1 closed (2026-05-26):** **`desktop/commands.rs` 不拆分**（~1.5k，维护者决定；避免碎片化）；Assessment §1 **10/10** 架构定型；`commands.rs` 模块注释。
- **Architecture / D8 (2026-05-26):** OpenAPI 3.1 导出 + `web-ui` TS 自动生成 — [`D8_OPENAPI_TS_GENERATION.md`](docs/tech/adr/D8_OPENAPI_TS_GENERATION.md)；`export-runtime-openapi` + [`zagens-runtime-v1.openapi.json`](docs/tech/openapi/zagens-runtime-v1.openapi.json)；`openapi-typescript` → `web-ui/src/api/generated/runtime-api.ts` + [`runtimeTypes.ts`](crates/desktop/web-ui/src/api/runtimeTypes.ts)；`/v2` 草案 [`V2_API_VERSIONING.md`](docs/tech/adr/V2_API_VERSIONING.md)。Assessment §1 **9/10**。
- **Docs / Harness integration proposal (2026-05-26):** [`HARNESS_INTEGRATION_PROPOSAL.md`](docs/tech/adr/HARNESS_INTEGRATION_PROPOSAL.md) — 把 [`docs/Agent+Harness组合式编程方案.md`](docs/Agent+Harness组合式编程方案.md) v1.2 远景方案落到 Zagens 现状：名词映射表（黑板/任务图/决策日志/笔记 → scratchpad + CRAFT + plan/todo/tasks + topic-memory + execpolicy 等已有承载）、Phase 0–3 全部搭车 D6/D7/D8/D13（**零新主线**、零 Engine 字段新增、零新持久化轨、零新顶层 `/v1/*` 路径），§11–§12 数学基础 11 个模型降级/删除清单（粗糙集 → TOML 规则表；贝叶斯/SPRT/指数衰减 → 删除；可能性论 → UI 文案标签；进化引擎 → 延期 v1.0+）。Status: **Proposed**；§1 不变（7/10）。
- **Architecture / D7 complete (2026-05-26):** [`D7_PERSISTENCE_UNIFICATION.md`](docs/tech/adr/D7_PERSISTENCE_UNIFICATION.md) — C1 `runtime_thread_id` SQLite；C2 [`PERSISTENCE.md`](docs/tech/PERSISTENCE.md)；C3 resume 集成测；C4 `deepseek thread list --source runtime`；C5 删除 `app-server`；§1 **8/10**。
- **Architecture / D9 + D10 (2026-05-26):** [`D9_D10_DESKTOP_UX.md`](docs/tech/adr/D9_D10_DESKTOP_UX.md) — `turnControl.ts` 两层 Stop 契约；`filterThreadStreamEvents` + `windowOwnsThreadForStream` 消除多窗口幽灵 SSE；API_DESIGN §2.1.1–2.1.2。§1 仍 **7/10**（体验债）。
- **Architecture / D6 (2026-05-26):** [`D6_RUNTIME_SERVER.md`](docs/tech/adr/D6_RUNTIME_SERVER.md) — 新增 `crates/runtime-server` + 二进制 **`deepseek-runtime`**（不链 ratatui）；`deepseek-tui` 特性 **`tui-ui`** 门控 TUI 栈；共享模块 `agent_surface` / `auto_route` / `context_reference` / `runtime_serve`；Zagens `externalBin` → `deepseek-runtime-*`；Assessment §1 #5 勾选（**7/10**）。
- **Architecture / D4 (2026-05-26):** [`app-server` 实验栈标记 deprecated](docs/tech/adr/D4_APPSERVER_DEPRECATED.md) — 决策 ADR、Assessment §1 #7 勾选；`deepseek app-server` CLI help、`deepseek-app-server` crate 文档 + `#[deprecated]` on `run`/`run_stdio`；**crate 代码移除 defer**。
- **Docs / architecture assessment (2026-05-26):** [`ARCHITECTURE_ASSESSMENT_2026-05-25.md`](docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md) M7/M8 复评 + D4 — 进度 **6/10**；D5 ✅；下一优先 D6 `runtime-server`。
- **Docs / architecture assessment (2026-05-26):** [`ARCHITECTURE_ASSESSMENT_2026-05-25.md`](docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md) 新增 **§5.1 推荐实施顺序**（维护者签收）— 主线 D6→D9/D10→D7→D8→D1→P2；§0 摘要与冻结窗口估算同步；§5 表注明「已记账 ≠ 已落地」。[`RUNTIME_EVOLUTION_ROADMAP.md`](docs/tech/RUNTIME_EVOLUTION_ROADMAP.md) 头部交叉引用同步（6/10 + §5.1）。

- **Runtime / M-series M8 (PR_M0 §6 M8):** Final strangler step — core **`Engine::run()`** op loop lands in `zagens-core::engine::op_loop` (cancel / approve / deny / truncate handled core-side; platform ops via `EnginePlatformExt`). Tui `EngineRuntimeExt` implements dispatch in `platform_dispatch.rs`; `op_loop.rs` + `op_handlers.rs` deleted from tui. **`Engine::ext`** is now `Box<dyn EnginePlatformExt<P,R>>` (was `Box<dyn Any>`). Pre-existing engine integration tests **`refresh_system_prompt_under_capacity_omits_topic_memory_block`** (3× `on_turn_complete` fixture) and **`engine_mock_capacity_pre_request_observes_mock_and_emits_decision`** green after partition-trim bulk fast-path. Closes [BACKLOG_ENGINE_STRUCT_IN_CORE.md](docs/tech/adr/BACKLOG_ENGINE_STRUCT_IN_CORE.md); deletes [HANDOFF_M7_M8.md](docs/tech/adr/HANDOFF_M7_M8.md).

- **Runtime / M-series M7 (PR_M0 §6 M7):** Seventh strangler step — `Engine` struct + `Engine::with_hosts` + tui `build_engine` builder land in `zagens-core`; tui keeps a **newtype wrapper** (`#[repr(transparent)] Engine(pub(crate) core::Engine<…>)`) so inherent impls / `TurnLoopHost` remain legal. Host fields swap to trait objects; concrete handles + `EngineConfigExt` live in `EngineRuntimeExt` behind `EnginePlatformExt`. `engine_new.rs` deleted; `spawn_engine` → `build::build_engine`. Shim split: `engine.rs` (~130 LOC) + `prelude_uses.rs` include.

- **Runtime / M-series M6 (PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE §3 rows #20 + #22, §5 R10, §6 M6; ARCHITECTURE_ASSESSMENT §1 #4, §3.4):** Sixth strangler step — `CapacityController` (677-LOC body) + the M1-deferred coherence reducer (`CoherenceSignal` + `next_coherence_state`) move atomically into `zagens-core` per spike R10 ("single atomic move, delete tui copy in same PR; no double-implementation period"). Zero behavior change — pure type move.
  - **`crates/core/src/capacity.rs`** (41 → 706 LOC): `CapacityControllerConfig` (already there since P2 PR4) is joined by the full controller surface — `GuardrailAction` (4 variants), `RiskBand` (3 variants), `CapacityObservationInput`, `DynamicSlackProfile`, `CapacitySnapshot`, `CapacityDecision`, `GuardrailRuntimeState` (private), `CapacityController` (with `new`, `observe_pre_turn`, `observe_post_tool`, `decide`, `mark_turn_start`, `mark_intervention_applied`, `mark_replay_failed`, `last_snapshot`, private `observe` + `model_prior`), `decide_policy(config, snapshot) -> GuardrailAction` free fn, and the math/window helpers (`normalize_model_prior_key`, `log2_1p`, `push_window`, `compute_profile`, `sigmoid`). 12 unit tests + 1 `#[ignore]` microbench (`bench_compute_profile`) move with the body — only the tui-`Config`-coupled `app_config_without_capacity_uses_default_disabled` test stays tui-side along with its adapter.
  - **`crates/core/src/coherence.rs`** (39 → 157 LOC): `CoherenceState` ladder enum (P2 PR4) is joined by `CoherenceSignal` enum (5 variants) + `next_coherence_state(current, signal) -> CoherenceState` reducer + the `synthetic_capacity_event_log_drives_plain_language_ladder` log-replay unit test. The reducer references `super::capacity::{GuardrailAction, RiskBand}` locally — this dependency is exactly why M1 (spike row #22) deferred the reducer: it could only land after `capacity::{GuardrailAction, RiskBand}` themselves landed in core, which happens in this same commit.
  - **`crates/tui/src/core/capacity.rs`** (677 → 102 LOC, net −575 LOC): pure re-export shim — `pub use zagens_core::capacity::{CapacityController, CapacityControllerConfig, CapacityDecision, CapacityObservationInput, CapacitySnapshot, DynamicSlackProfile, GuardrailAction, RiskBand, decide_policy};`. Keeps only `capacity_config_from_app(config: &crate::config::Config) -> CapacityControllerConfig` (the tui-side adapter that projects the flat `crate::config::Config` onto the core controller config — stays tui because the type cannot cross the layering boundary) and its single unit test.
  - **`crates/tui/src/core/coherence.rs`** (102 → 14 LOC, net −88 LOC): pure re-export shim — `pub use zagens_core::coherence::{CoherenceSignal, CoherenceState, next_coherence_state};`. All 15 call sites under `crates/tui/src/core/engine/capacity_flow/*`, `crates/tui/src/runtime_threads/*`, `crates/tui/src/tui/ui*`, `crates/tui/src/tui/widgets/mod.rs`, `crates/tui/src/cli/commands/legacy.rs`, and the engine state (`tui::core::engine::types::EngineConfig`) keep compiling unchanged — **zero Engine call-site swaps required** (the type-move semantics let the shims handle the entire fan-out).
  - **Not in M6 scope (intentional):** `crates/tui/src/core/capacity_memory.rs` (286 LOC) — disk persistence (`save_metrics`, `load_metrics`, JSONL append fallback chain) is an engine-flow concern, not part of spike row #20's controller field, and uses no tui-only deps beyond `crate::config` paths from its callers — can opportunistically move later if M7/M8 needs it. `crates/tui/src/core/engine/capacity_flow/*` (5 files, ~1.3k LOC of engine-side checkpoints / replay / interventions / persistence / observation orchestration) stays tui until M7 (Engine struct migration) — they own `&mut Engine` state and depend on tui-side messaging plumbing. `crates/tui/src/core/engine/turn_loop/host_impl/capacity.rs` (turn-loop host impl, ~80 LOC) similarly stays until M7.
  - Net diff `git diff --stat HEAD~..HEAD`: 4 files (2 in core, 2 in tui), 858 insertions / 783 deletions — **~+75 LOC net** (cap ≤700; verbatim type move shifts code rather than adding it). Acceptance per spike §6 M6: `core --lib capacity` 11/11 + bench ignored 1, `core --lib coherence` 1/1, `core --lib engine::turn_loop::capacity_policy` 4/4, `tui --lib capacity_escalation` 2/2, `tui --lib coherence` (footer chip) 1/1, `tui --lib core::capacity_memory` 3/3, `tui --lib capacity_disabled_by_default_keeps_messages_intact` ok, `tui --lib seam_manager` 7/7 ok, `tui --lib mcp` 36/36 ok, `tui --lib tools::subagent` 108/108 ok, `tui --lib runtime_api::tests::sidecar_contract_full_lifecycle` ok, `tui --lib history_isomorphism` 9/9 ok, `tui --test protocol_recovery` 9/9 ok, `cargo build -p deepseek-{core,tui}` clean, `npm run test:f3 && npm run build` (web-ui) clean. The same 2 pre-existing `core::engine::tests::{refresh_system_prompt_under_capacity_omits_topic_memory_block, engine_mock_capacity_pre_request_observes_mock_and_emits_decision}` failures **persist with identical line numbers (tests.rs:991 / tests.rs:2452) and assertion text** as on M3/M4/M5 HEAD — confirmed unrelated to M6 (which is zero-behavior-change type move). I had hypothesized M6 might fix the `engine_mock_capacity_pre_request_observes_mock_and_emits_decision` failure since the test exercises the capacity decision path, but the persistence confirms the bug is in engine-flow wiring (`capacity_flow/observation.rs` or similar) rather than in `CapacityController` itself — that bug is M7 territory (Engine struct + engine-flow integration).
  - Promotes [BACKLOG_ENGINE_STRUCT_IN_CORE.md](docs/tech/adr/BACKLOG_ENGINE_STRUCT_IN_CORE.md) Progress table: M6 row `queued` → `landed`. M7–M8 remain queued per [PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md](docs/tech/adr/PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md) §6.

- **Runtime / M-series M5 (PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE §3 rows #21 + #25 + #28-#31, §5 R1 + R9 + R12, §6 M5; ARCHITECTURE_ASSESSMENT §1 #4, §3.4):** Fifth strangler step — three subsystem host traits (`SeamHost` / `WorkshopHost` / `TopicMemoryHost`) plus the small `ScratchpadStepState` type migrate into `zagens-core`. The heavy implementations stay tui-side: `seam_manager.rs` (712 LOC), `large_output_router.rs` (604 LOC), `topic_memory.rs` (307 LOC) and `scratchpad_flow.rs` (484 LOC of UI / auditor / coverage helpers) are **not** moved — spike §5 R12 ("scratchpad belongs UI-side") + R9 ("prefer adapter-tui-side over pulling `zagens-topic-memory` into core deps") honored.
  - New `zagens_core::engine::hosts::seam::SeamHost` — widest M-series trait so far (10 methods, the entire layered-context Flash pipeline #159):
    - `config_enabled()` / `highest_level()` / `seam_level_for(active_input_tokens, highest_existing_level)` / `verbatim_window_start(message_count)` (pre-request checkpoint decision surface).
    - `collect_seam_texts(messages)` / `produce_soft_seam(messages, level, start_idx, end_idx, workspace, pinned_indices)` / `recompact(existing_seams, recent, level, start_idx, end_idx)` (seam production).
    - `seam_count()` / `produce_flash_briefing(existing_seams, state_text)` / `reset()` (cycle bookkeeping).
    - Opaque `SeamError = Box<dyn std::error::Error + Send + Sync + 'static>` so `anyhow::Error` widens via `.map_err(Into::into)` without leaking the tui-side `anyhow` / `LlmClientError` hierarchy through the core trait surface. `Display` blanket of `dyn Error` preserves the existing log shape (`cycle_hooks.rs` / `layered_context.rs` already format with `{err}`).
    - Strictly call-graph driven (R1): inherent `SeamManager` methods `new` (construction is tui's `LlmClient`-factory concern), `should_cycle` (currently dead code), and the private `summarize_messages` helper are **deliberately not on the trait**. `config(&self) -> &SeamConfig` is replaced by the narrower `config_enabled() -> bool` accessor — `SeamConfig` is a tui-only type and Engine only reads `.enabled`.
  - New `zagens_core::engine::hosts::workshop::WorkshopHost` — **empty marker** (mirrors M3 `ShellHost`). Engine never invokes a method on `workshop_vars`; the single call site at `tool_context.rs:51` only clones the `Arc<Mutex<WorkshopVariables>>` into `ToolContext` (every `WorkshopVariables` method is called from inside tool implementations, not from Engine). `crates/tui/src/tools/large_output_router.rs` adds `pub struct TuiWorkshopHost(pub Option<Arc<Mutex<WorkshopVariables>>>)` newtype + empty `impl WorkshopHost` per R1.
  - New `zagens_core::engine::hosts::topic_memory::TopicMemoryHost` — 2 methods (`compose_block(query_hint) -> Option<String>` / `on_turn_complete(user, assistant)`). **Settings move into the implementation** (`TopicMemoryRuntime` gains an owned `settings: TopicMemorySettings` field at construction; new `TopicMemoryRuntime::new(settings)` constructor) so the trait surface stays settings-free — avoids both spike R9 option (b) (adding `zagens-topic-memory` to core deps) and the parallel-settings-struct anti-pattern. Settings hot-reload is not currently exposed via any slash command, so single-shot ownership at engine init is sufficient.
  - New `zagens_core::engine::scratchpad_state::ScratchpadStepState` — small state struct (~30 LOC, 2 `usize` fields + `reset(&mut self)`) per spike §3 row #28 + R12. The heavy `crates/tui/src/core/engine/scratchpad_flow.rs` (484 LOC of audit/coverage/reminder helpers — `record_tool_outcome`, `inject_summary_if_needed`, `build_layered_summary`, `coverage_gate`, `read_inventory`, …) **stays tui-side**; the file keeps a `pub use zagens_core::engine::ScratchpadStepState;` re-export shim so every existing `use crate::core::engine::scratchpad_flow::ScratchpadStepState` caller (engine state, `host_impl/mod.rs` turn-loop bookkeeping, `message_handlers.rs` reset, tests) compiles unchanged.
  - tui inline trait impls (one per host, no extra files):
    - `impl SeamHost for SeamManager` (in `crates/tui/src/seam_manager.rs`) — 10 thin UFCS delegations to the existing inherent methods; errors widened via `.map_err(Into::into)`.
    - `impl WorkshopHost for TuiWorkshopHost` (in `crates/tui/src/tools/large_output_router.rs`) — empty body.
    - `impl TopicMemoryHost for TopicMemoryRuntime` (in `crates/tui/src/topic_memory.rs`) — both methods clone `self.settings` to side-step the `&mut self + &self.settings` simultaneous borrow that `compose_block`'s legacy inherent signature requires (`TopicMemorySettings` is cheap to clone: `bool + PathBuf + u32 + usize + Option<String>`).
  - Engine call-site swaps (proves the trait surface actually covers Engine's needs):
    - `crates/tui/src/core/engine/layered_context.rs` — 8 `seam_mgr.method(...)` calls → `SeamHost::method(seam_mgr, ...)` via a `use zagens_core::engine::hosts::SeamHost;` at the top of the function module. `seam_mgr.config().enabled` → `SeamHost::config_enabled(seam_mgr)`.
    - `crates/tui/src/core/engine/cycle_hooks.rs` — `collect_seam_texts` / `produce_flash_briefing` / `reset` in the cycle-advance path; `topic_memory_runtime.compose_block(&self.config.topic_memory, query_hint)` → `TopicMemoryHost::compose_block(&mut self.topic_memory_runtime, query_hint)` (settings now owned by the runtime).
    - `crates/tui/src/core/engine/message_handlers.rs` — `topic_memory_runtime.on_turn_complete(&self.config.topic_memory, user, assistant)` → `TopicMemoryHost::on_turn_complete(&mut self.topic_memory_runtime, user, assistant)`.
    - `crates/tui/src/core/engine/engine_new.rs:207` — `TopicMemoryRuntime::default()` → `TopicMemoryRuntime::new(topic_memory_settings)` (settings clone-owned at engine init).
    - `crates/tui/src/core/engine/tests.rs:974` — same constructor swap.
  - **Skipped in M5 (intentional, per call-graph audit + R12):** the field types themselves stay tui (`Option<SeamManager>` / `Option<Arc<Mutex<WorkshopVariables>>>` / `TopicMemoryRuntime`); M7 will swap them to `Option<Box<dyn SeamHost>>` etc. when the core `Engine` struct lands. The `scratchpad_flow.rs` 484 LOC of UI/auditor helpers + `seam_manager.rs` 712 LOC + `topic_memory.rs` 307 LOC body stay tui-side per R12.
  - Net diff `git diff --stat HEAD~..HEAD`: core +289 (new `hosts/seam.rs` ~126, `hosts/workshop.rs` ~41, `hosts/topic_memory.rs` ~60, `scratchpad_state.rs` ~62 incl. 2 unit tests); tui +252/−48 (4 inline trait impls + engine call-site swaps + scratchpad re-export shim + tests update) = **~+493 LOC net** (cap ≤700). Acceptance per spike §6 M5: `core --lib engine::scratchpad_state` 2/2 ok, `tui --lib seam_manager` 7/7 ok, `tui --lib compaction` 66/66 ok, `tui --lib scratchpad` 25/25 ok, `tui --lib tools::subagent` 108/108 ok, `tui --lib mcp` 36/36 ok, `tui --lib runtime_api::tests::sidecar_contract_full_lifecycle` ok, `tui --lib history_isomorphism` 9/9 ok, `tui --test protocol_recovery` 9/9 ok, `cargo build -p deepseek-{core,tui}` clean, `npm run test:f3 && npm run build` (web-ui) clean. The pre-existing `core::engine::tests::{refresh_system_prompt_under_capacity_omits_topic_memory_block, engine_mock_capacity_pre_request_observes_mock_and_emits_decision}` failures **persist on M5 HEAD with the identical assertion line / failure mode** as on M3 and M4 HEAD — confirmed unrelated to M5's 3-trait + scratchpad diff (those tests touch the topic-memory injection cadence and capacity-controller path; the assertion fires before any M5 trait dispatch is exercised).
  - Promotes [BACKLOG_ENGINE_STRUCT_IN_CORE.md](docs/tech/adr/BACKLOG_ENGINE_STRUCT_IN_CORE.md) Progress table: M5 row `queued` → `landed`. M6–M8 remain queued per [PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md](docs/tech/adr/PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md) §6.

- **Runtime / M-series M4 (PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE §3 row #8, §6 M4; ARCHITECTURE_ASSESSMENT §1 #4, §3.4):** Fourth strangler step — `McpHost` trait promotes the empty `TurnLoopMcpPool` marker into a named host trait alongside M3's `LspHost` / `SubAgentHost` / `ShellHost` / `SandboxHost`. **Hard constraint honored (spike §6 M4):** zero changes to `crates/tui/src/mcp.rs` body (2218 LOC) — every method is a default impl delegating to the existing free functions in `core::engine::dispatch`. Net diff well under the ≤500 LOC cap.
  - New `zagens_core::engine::hosts::mcp::McpHost` (and re-export at `zagens_core::engine::McpHost`) — 4 default-impl methods covering the live engine's MCP predicate / metadata surface:
    - `is_mcp_tool(&self, name) -> bool` — delegates to new `core::engine::dispatch::is_mcp_tool_name(name)` free fn (mirrors the body of `tui::mcp::McpPool::is_mcp_tool` so the core turn loop can answer the same question without a tui dependency).
    - `tool_is_parallel_safe(&self, name)` — delegates to `core::engine::dispatch::mcp_tool_is_parallel_safe`.
    - `tool_is_read_only(&self, name)` — delegates to `core::engine::dispatch::mcp_tool_is_read_only`.
    - `tool_approval_description(&self, name)` — delegates to `core::engine::dispatch::mcp_tool_approval_description`.
  - `TurnLoopMcpPool` deprecation cycle: the marker stays in `core::engine::turn_loop::host` as a `#[deprecated(since = "0.8.16", note = "use zagens_core::engine::hosts::McpHost instead")]` alias with a blanket `impl<T: McpHost + ?Sized> TurnLoopMcpPool for T {}` so existing `Self::McpPool: TurnLoopMcpPool` bounds keep building for one release. `TurnLoopHost::McpPool` associated-type bound changed from `TurnLoopMcpPool` to `McpHost`. `pub use host::TurnLoopMcpPool` in `turn_loop::mod.rs` carries `#[allow(deprecated)]` so the internal re-export does not warn.
  - tui swap: `impl TurnLoopMcpPool for McpPool {}` (`crates/tui/src/core/engine/turn_loop/host_impl/mod.rs:42`) → `impl McpHost for McpPool {}` (one-liner; uses default impls only — `McpPool` has no extra state to override). `McpPoolPort` dispatch trait (P2 PR4) and `McpPoolHandle = Arc<Mutex<McpPool>>` wrapper are **unchanged** — they own a different `self` shape (locked container vs. bare pool) and stay orthogonal to `McpHost`.
  - Drift-guard tests (M4 Q5A "zero call-site churn" mitigation — the tui inherent `McpPool::is_mcp_tool` and the core free fn `is_mcp_tool_name` are dual definitions per the spike's "zero changes to mcp.rs body" constraint):
    - `core::engine::dispatch::tests::is_mcp_tool_name_covers_prefix_and_resource_helpers` (8 names).
    - `tui::core::engine::turn_loop::host_impl::m4_drift_guard::is_mcp_tool_name_matches_tui_mcp_pool` — asserts the two definitions produce identical output on a 15-name curated set spanning `mcp_*` prefix, the three `*_mcp_resource*` literals, and known non-MCP names (`read_file`, `exec_shell`, …).
    - `tui::core::engine::turn_loop::host_impl::m4_drift_guard::mcp_pool_satisfies_mcp_host_with_default_impls` — type-level bound assertion `McpPool: McpHost`.
    - `core::engine::hosts::mcp::tests::default_impls_match_dispatch_module` + `dyn_dispatch_compiles` — stub-host coverage of the four default methods.
  - **Skipped in M4 (intentional, per call-graph audit + spike §5 R1):** `execute_tool` (lives on `McpPoolPort`, implemented on `McpPoolHandle = Arc<Mutex<McpPool>>` — different `self` shape; merging would require reworking the `mcp_pool_as_port` factory and rippling through every `Option<Arc<AsyncMutex<Self::McpPool>>>` turn-loop parameter); `ensure_pool` / `shutdown_all` (mutate engine state — `self.mcp_pool = Some(...)` — and depend on `EngineConfigExt.network_policy` + `session.mcp_config_path`; stay as inherent `Engine` methods at `tool_context.rs:112-124` and `op_loop.rs:86-89`, will move into the core `Engine` struct alongside the field in M7).
  - Net diff `git diff --stat HEAD~..HEAD`: core +185 (new `hosts/mcp.rs` ~125, `dispatch.rs` +31, host.rs +18, mod.rs/hosts.rs +10, turn_loop/mod.rs +4); tui +73/−12 (impl swap + drift guard); docs +30 = **~+275 LOC net** (cap ≤500). Acceptance per spike §6 M4: `core --lib engine::hosts::mcp` 2/2 ok, `core --lib engine::dispatch` 4/4 ok, `tui --lib mcp` 36/36 ok (includes `test_mcp_pool_is_mcp_tool` + 2 M4 drift-guard tests + `mcp_pool_handle_implements_core_mcp_port` P2 PR4 trait satisfaction), `tui --lib m4_drift_guard` 2/2 ok, `tui --lib tools::subagent` 108/108 ok, `tui --lib runtime_api::tests::sidecar_contract_full_lifecycle` ok, `tui --lib history_isomorphism` 9/9 ok, `tui --test protocol_recovery` 9/9 ok, `core --lib engine::turn_loop::capacity_policy` 4/4 ok, `cargo build -p deepseek-{core,tui}` clean, `npm run test:f3 && npm run build` (web-ui) clean. The 2 `core::engine::tests::{refresh_system_prompt_under_capacity_omits_topic_memory_block, engine_mock_capacity_pre_request_observes_mock_and_emits_decision}` failures observed on the working tree were **independently reproduced on M3 HEAD (`1db7a51`)** and confirmed pre-existing — unrelated to M4's MCP-only diff (both tests touch the topic-memory / capacity-controller path, not MCP).
  - Promotes [BACKLOG_ENGINE_STRUCT_IN_CORE.md](docs/tech/adr/BACKLOG_ENGINE_STRUCT_IN_CORE.md) Progress table: M4 row `queued` → `landed`. M5–M8 remain queued per [PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md](docs/tech/adr/PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md) §6.

- **Runtime / M-series M3 (PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE §3 rows #6–#9 + #26–#27, §6 M3; ARCHITECTURE_ASSESSMENT §1 #4, §3.4):** Third strangler step — subsystem host traits (LspHost / SubAgentHost / ShellHost / SandboxHost) introduced + supporting data types moved into `zagens-core`. **Strictly call-graph driven** (spike §5 R1): each trait method exists iff the live `Engine` calls it — pass-through fields (Shell, Sandbox) get marker / single-accessor traits so M7 only needs to swap the field type, not invent new surface.
  - New `zagens_core::engine::hosts::{LspHost, SubAgentHost, ShellHost, SandboxHost}` — engine boundary trait module. `LspHost` (2 methods: `enabled()` + `diagnostics_for(file, edit_seq) -> Option<DiagnosticBlock>`); `SubAgentHost` (3 methods: `spawn_general()`, `list_with_cleanup()`, `running_count()`); `ShellHost` empty marker (Engine never invokes shell methods directly — only clones the `SharedShellManager` into `ToolContext`); `SandboxHost` single accessor `backend() -> Option<&Arc<dyn SandboxBackend>>` (Engine only forwards the optional `Arc` to `ToolContext`).
  - Data-type moves into core (matching spike §3 rows #26 + #27):
    - `zagens_core::lsp::diagnostics` ← `tui::lsp::diagnostics` — `Diagnostic` / `DiagnosticBlock` / `Severity` / `render_blocks` + 8 unit tests. Pure `std::path` deps. The tui crate keeps `tui::lsp::diagnostics` as a re-export shim so existing `crate::lsp::DiagnosticBlock` / `crate::lsp::render_blocks` callers (engine tests, `tools/spec.rs`) compile unchanged.
    - `zagens_core::sandbox` (new top-level module) ← `tui::sandbox::backend` (trait + types only) — `SandboxBackend` trait, `SandboxOutput`, `SandboxKind` + `SandboxKind::parse` / `as_str` + 2 unit tests. The tui `create_backend(&Config)` factory and the `OpenSandboxBackend` impl stay tui-side (depend on tui's `Config`); `tui::sandbox::backend` re-exports the trait/types from core so `use crate::sandbox::backend::SandboxBackend` etc. keep working.
  - Trait implementations (inline on existing tui types per Q3 decision):
    - `impl LspHost for crate::lsp::LspManager` — delegates `enabled()` to `config.enabled` and `diagnostics_for(...)` to the existing inherent method via UFCS.
    - `impl SubAgentHost for Engine` — replaces the old `impl SubAgentSpawnPort for Engine` (orchestration unchanged: still calls into `Engine::spawn_general_subagent` / `Engine::list_subagents`); adds `running_count` (reads `subagent_manager.read().await.running_count()`).
    - `crate::sandbox::TuiSandboxHost(pub Option<Arc<dyn SandboxBackend>>)` newtype + `impl SandboxHost` — mirrors the `SharedShellManager` ownership pattern.
    - `crate::tools::shell::TuiShellHost(pub SharedShellManager)` newtype + empty `impl ShellHost` (bare `ShellManager` is `Send` but not `Sync` — it holds `Box<dyn Write + Send>` and `Box<dyn portable_pty::Child + Send>` fields — so the trait is implemented on the `Arc<Mutex<...>>`-shaped newtype instead of the raw manager).
  - `SubAgentSpawnPort` → `SubAgentHost` rename: the old trait stays in `zagens_core::engine::subagent_port` as a `#[deprecated(since = "0.8.16", note = "use zagens_core::engine::hosts::SubAgentHost instead")]` alias so any out-of-tree consumers (none in this workspace) keep building for one cycle. `pub use SubAgentSpawnPort` in `engine::mod.rs` carries `#[allow(deprecated)]` so the internal re-export does not warn.
  - Engine call-site swaps (proves the trait surface actually covers Engine's needs):
    - `crates/tui/src/core/engine/lsp_hooks.rs:24,38` — `self.lsp_manager.config().enabled` / `.diagnostics_for(...)` → `LspHost::enabled(&*self.lsp_manager)` / `LspHost::diagnostics_for(...)` via a `&dyn LspHost` reborrow.
    - `crates/tui/src/core/engine/turn_loop/host_impl/no_tool_uses.rs:68` — `self.subagent_manager.read().await.running_count()` → `<Engine as SubAgentHost>::running_count(self).await`.
  - Net diff (estimated `git diff --stat HEAD~..HEAD`): core +460 (new `sandbox/mod.rs`, `lsp/diagnostics.rs`, `engine/hosts/{mod,lsp,subagent,shell,sandbox}.rs`); tui −265/+90 (shim + impls + newtypes + call-site swaps); docs +20 = **~+320 LOC net** (cap ≤700). Acceptance per spike §6 M3: `core --lib lsp/sandbox` ok (11+2 new tests), `tui --lib tools::subagent` 108/108 ok, `tui --lib history_isomorphism` ok, `core --lib capacity_policy` ok, `tui --lib config::tests::instructions_paths` ok, `tui --lib tools::subagent::tests::resident_file` ok, `tui --lib core::engine::tests::build_tool_context_wires_lsp` ok, `tui --lib capacity_escalation` ok, `tui --test protocol_recovery` 9/9 ok, `tui --lib sidecar_contract_full_lifecycle` ok, `cargo build -p deepseek-{core,tui}` clean.
  - Promotes [BACKLOG_ENGINE_STRUCT_IN_CORE.md](docs/tech/adr/BACKLOG_ENGINE_STRUCT_IN_CORE.md) Progress table: M3 row `queued` → `landed`. M4–M8 remain queued per [PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md](docs/tech/adr/PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md) §6.

- **Runtime / M-series M2 (PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE §3 row #1, §5 R2, §6 M2; ARCHITECTURE_ASSESSMENT §1 #4):** Second strangler step — `EngineConfig` split type pillars established. The fat tui `EngineConfig` (30 fields) is now conceptually `lean (25) ⊕ ext (5/8)`:
  - New `zagens_core::engine::config::EngineConfig` — **lean** 25-field subset depending only on core types (`model/workspace/allow_shell/trust_mode/notes_path/mcp_config_path/skills_dir/instructions/max_steps/max_subagents/subagent_step_timeout/features/compaction/cycle/capacity/max_spawn_depth/snapshots_enabled/subagent_model_overrides/memory_enabled/memory_path/goal_objective/locale_tag/strict_tool_mode/task_type/scratchpad`). Plain `Default` lands placeholder paths (`PathBuf::new()` for `skills_dir`, `model = ""`) since the tui facade owns the disk-aware defaults; core-only callers will override before use.
  - New `tui::core::engine::types::EngineConfigExt` — **ext** carry for the 8 tui-only fields (`todos/plan_state/network_policy/lsp_config/runtime_services/topic_memory/workshop/llm_client_override`). Marked `#[allow(dead_code, reason = "M2 type pillar — first consumer lands in M3")]` because production code still flows through the monolithic facade.
  - `tui::core::engine::types::EngineConfig` keeps its **flat 30-field layout** so every existing caller (≈30 literal-construction sites in `core::engine::tests`, `cli/commands/legacy.rs`, `runtime_threads/engine_load.rs`, etc.) compiles unchanged. Four new accessors carve the projection: `lean(&self) -> core::EngineConfig`, `ext(&self) -> EngineConfigExt`, `into_parts(self) -> (lean, ext)`, `from_parts(lean, ext) -> Self`. Two round-trip unit tests (`lean_into_parts_round_trip`, `lean_borrow_matches_into_parts_owned`) guarantee the projection stays aligned as fields evolve.
  - **Why facade over `Engine::new(slim, ext_via_host)` now:** spike R2's two-arg signature would force ~30 literal-construction sites to rewrite to `EngineConfig { core: core::EngineConfig { … }, ext: EngineConfigExt { … } }`, blowing the ≤700 LOC cap. M2 stops at type pillars; M7 (Engine struct → core) will atomically switch the entry point to `Engine::with_hosts(lean, ext)` once the host trait surface from M3–M6 is in place.
  - Net diff `git diff --stat HEAD~..HEAD`: `crates/core/src/engine/config.rs` +119 (new), `crates/core/src/engine/mod.rs` +1, `crates/tui/src/core/engine/types.rs` +259/−1 = **+378 LOC net** (cap ≤700). Acceptance per spike §6 M2: `engine_llm_client_override_runs_mock_turn` ok, 36 `error_taxonomy` golden ok (core suite), `sidecar_contract_full_lifecycle` ok, 2/2 new round-trip tests ok, `cargo check --workspace --all-targets` clean, `npm run test:f3` clean.
  - Promotes [BACKLOG_ENGINE_STRUCT_IN_CORE.md](docs/tech/adr/BACKLOG_ENGINE_STRUCT_IN_CORE.md) Progress table: M2 row `pending` → `landed`. M3–M8 remain queued per [PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md](docs/tech/adr/PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md) §6.

- **Runtime (D2 follow-up / ARCHITECTURE_ASSESSMENT §3.3):** Sidecar now accepts `--port 0` (OS-assigned ephemeral port). Removed the `if options.port == 0 { bail!("Port must be > 0"); }` guard in `crates/tui/src/runtime_api/mod.rs`; the rest of the chain (`TcpListener::bind` + `listener.local_addr().port()` + `DS_PICK_READY {port: <bound>}` + desktop `watch::Receiver<u16>` consumer) was already in place from the D2 infrastructure commit (`4d1cbab`). `bail!` removed from `anyhow` import (no other callers). `sidecar_contract_full_lifecycle` re-run green. Closes the remaining "one-liner" follow-up from the prior D2 ChangeLog entry.

- **Runtime / M-series M1 (PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE §7, ARCHITECTURE_ASSESSMENT §1 #6):** First strangler step of the `Engine` struct → `zagens-core` migration. Three carrier types moved into `crates/core/src/engine/` with **behavior-only** changes (no `/v1` wire format change, no sidecar contract change):
  - `Op` enum (15 variants) → `zagens_core::engine::op::Op`. `Op::SendMessage.mode` / `Op::ChangeMode.mode` now use `core::turn::TurnLoopMode` instead of `tui::AppMode` (1:1 isomorphic Agent/Yolo/Plan). Producers wrap via `app_mode_to_turn_loop(...)` (5 sites: `tui/ui.rs`, `cli/commands/legacy.rs`, `tests.rs` x3); the dispatch loop unwraps via `turn_loop_to_app_mode(...)` once so all tui-side `handle_*_op` signatures stay untouched.
  - `EngineHandle` → `zagens_core::engine::handle::EngineHandle<P, R>` — generic over sandbox policy (`P`) and `request_user_input` response (`R`); `P, R: Send + Sync + 'static`. tui crate keeps `pub type EngineHandle = ...<SandboxPolicy, UserInputResponse>;` alias so all 18 caller import paths stay intact. New `pub fn EngineHandle::new(...)` replaces the prior `pub(super)` field-literal construction at the two build sites (`engine_new.rs:211`, `mock.rs:57`). `impl TurnEnginePort for EngineHandle<P, R>` lives in core now (orphan-rule clean); the tui-side `core/engine/turn_port.rs` is deleted. New `TurnLoopMode::from_setting("agent"/"yolo"/"plan")` mirrors `AppMode::from_setting` so the runtime-API string ↔ enum boundary stays in core.
  - `ThreadContextSnapshot` struct → `zagens_core::engine::context_snapshot::ThreadContextSnapshot`. The `build_thread_context_snapshot` helper stays tui-side because it depends on the tui-only `compaction::should_compact` (~1k LOC) — M0 §1.2 keeps that out of scope.
  - tui re-export shims: `crate::core::ops` (1-line `pub use`), `crate::core::engine::handle` (single `type` alias), `crate::context_snapshot` (re-export + retained build helper) keep every existing import working.
  - **Skipped in M1 (intentional):** `coherence.rs` reducer — `CoherenceState` is already in core (P2 PR4); `next_coherence_state` + `CoherenceSignal` depend on tui-only `core::capacity::{GuardrailAction, RiskBand}` and will migrate together with `CapacityController` in M6 per spike §3 row #22.
  - Net diff `git diff --stat HEAD~..HEAD`: tracked +59/−329 + new core files +369 = **+99 LOC net** (cap was ≤ 500). Spike §7.4 acceptance checklist all green: 8/8 regression tests pass (`capacity_policy`, `history_isomorphism`, `instructions_paths`, `tools::subagent::resident_file`, `build_tool_context_wires_lsp`, `capacity_escalation`, `protocol_recovery`, `sidecar_contract_full_lifecycle`), web-ui `npm run test:f3` + `npm run build` green, `cargo check --workspace --all-targets` clean.
  - Promotes [BACKLOG_ENGINE_STRUCT_IN_CORE.md](docs/tech/adr/BACKLOG_ENGINE_STRUCT_IN_CORE.md) `In spike` → `In progress (M1 landed)`; M2–M8 remain queued per [PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md](docs/tech/adr/PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md) §6.

- **Zagens / Runtime (D2 / ARCHITECTURE_ASSESSMENT §1 #3, §3.3):** Runtime port discovery is now fully dynamic — desktop shell ↔ sidecar use `tokio::sync::watch::channel::<u16>` for port publishing, sidecar reports the **actually bound** port via `DS_PICK_READY.port` (was: the **requested** port, broken when binding to `0` / ephemeral). `AppContext::runtime_port` is `watch::Receiver<u16>`; new helpers `AppContext::{current_port,require_port}`; `get_runtime_port` IPC awaits `rx.changed()` until the first non-zero publish (web-ui `initRuntimeConfig` naturally serializes behind sidecar readiness). All call sites that previously used `ctx.runtime_port: u16` now go through `ctx.require_port()?` — covers `runtime_proxy::{http,post_stream,get_sse}`, `commands::{export_thread_json, export_session_json, rebuild_symbol_index, read_thread_workspace_binary}` (5 files: `desktop/src/{main,commands,runtime_proxy,sidecar}.rs` + `tui/src/runtime_api/mod.rs`). On restart paths supervisor `port_tx.send(0)` so IPC handlers either fast-fail (`require_port`) or await (`get_runtime_port`) instead of using stale ports. Initial suggested port stays `7878` for back-compat / `curl localhost:7878` debugging; the remaining "let sidecar bind `--port 0`" change is a one-liner (remove `if options.port == 0 { bail!(...) }`) that can ship in a follow-up PR. Regressions green: `sidecar_contract_full_lifecycle`, `desktop::architecture_boundary`, `desktop::runtime_proxy_paths`, full `cargo check --workspace --all-targets` clean.

### Removed

- **Workspace (D3 / ARCHITECTURE_ASSESSMENT §1 #8):** `crates/tui-core` legacy crate (event-driven TUI state machine scaffold + snapshot test) removed — confirmed not linked by any other crate (`deepseek-tui`, Zagens, CLI 均无 path 依赖). Workspace member removed from root `Cargo.toml`; directory deleted; `cargo check --workspace --all-targets` 全绿（3m37s, exit 0, 只剩 pre-existing dead_code warnings 与本次删除无关）. [RUNTIME_ARCHITECTURE.md](docs/tech/RUNTIME_ARCHITECTURE.md) §3 依赖图 legacy 节点同步移除；[ARCHITECTURE_ASSESSMENT_2026-05-25.md](docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md) §1 checklist 4/10、§3.8 标记完成、§5 P0 D3 行 ✅。

### Added

- **Runtime (A1 follow-up / §12.1 #1):** `App::check_live_history_isomorphism(site)` replaces the debug-only `debug_assert_live_history_isomorphism`; production builds also surface drift via `tracing::warn!(target = "tui::history_isomorphism")` + `history_isomorphism::drift_count()` (process-wide `AtomicU64`). Four call sites (`tool_complete` / `turn_complete` / `session_load` / `backtrack`) pass a static `site` label for triage. Regression tests `record_drift_increments_global_counter`, `reset_drift_count_for_test_zeroes_counter`, `drift_is_detected_when_tool_output_diverges` (serialized via per-module mutex). Roadmap §12.1 全 5 项闭合 — see [A1_PERSIST_BLOCKING_AUDIT.md](docs/tech/adr/A1_PERSIST_BLOCKING_AUDIT.md) §Status.
- **Docs:** Roadmap §17.5 / §12.1 #1 / §17.1 / §7.1 + [IMPLEMENTATION_SUMMARY_2026-05-24.md](docs/tech/adr/IMPLEMENTATION_SUMMARY_2026-05-24.md) sync for A1 live ToolCell isomorphism closure (2026-05-25).

### Fixed

- **Zagens desktop:** Right panel collapse state persists across restarts (`zagens-desktop-right-panel-collapsed`); first launch stays collapsed; sidebar inspector tabs expand the panel on click.

### Changed

- **Docs:** [A2_A3_SIGNOFF.md](docs/tech/adr/A2_A3_SIGNOFF.md) — §12.1 #2（Turn 可观测）与 #3（错误分类）维护者签收（2026-05-25）；路线图 §7.2/§7.3/§12.1 勾选同步。
- **Docs:** [RUNTIME_ARCHITECTURE.md](docs/tech/RUNTIME_ARCHITECTURE.md) 与代码对齐（2026-05-25）— P2 core/tui 拆分、crate 依赖图、`runtime_api/`/`runtime_threads/` 模块路径、双持久化/双通道、Zagens sidecar 监督、D12 Desktop-only。
- **Docs:** [RUNTIME_ARCHITECTURE.md](docs/tech/RUNTIME_ARCHITECTURE.md) 图表细化第二轮（2026-05-25）— §1 顶层系统总览拆为分层 subgraph（用户/桌面壳/sidecar/外部/持久化/CLI）并附"节点 ↔ 代码出处"映射表；§2 Sidecar 内部数据流细化（router→auth→stream/threads, manager 内 active/lifecycle/monitor/persist/broadcast 拆分）；§3 crate 依赖图与各 `Cargo.toml` 一一核对（含 `agent`/`execpolicy`/`hooks`/`protocol`/`state` 等所有真实边）；§5 双通道新增 mermaid 图 + `validate_runtime_path` 白名单 + SSE 取消 + sidecar 握手 `DS_PICK_READY`；§8 改为 sequenceDiagram 并补「Op 是 mpsc」「取消两层」要点；§9 关键模块索引扩到 16 条全 clickable 链接。
- **Docs:** 新增 [ARCHITECTURE_ASSESSMENT_2026-05-25.md](docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md) — 架构现状评估 + "先定型再迭代功能" 决策快照：§1 给出 10 条定型 checklist（当前 3/10）作为解冻判定门槛；§3 列出 10 项技术债（高/中/低）；§5 把迭代方向（D1–D14）按 P0/P1/P2 分级并交叉引用现有 backlog ADR（M-series PR_M0、RUNTIME_UNIFICATION、STATESTORE_JSONL、LANDLOCK_ENFORCE）；§7 落地"功能冻结期 PR 准入红线"（禁止在 `crates/tui` 新建顶层文件、禁止给 `Engine` struct 加新字段、禁止新增 `/v1/*` 无 OpenAPI schema 的端点等）。[RUNTIME_ARCHITECTURE.md](docs/tech/RUNTIME_ARCHITECTURE.md) 与 [RUNTIME_EVOLUTION_ROADMAP.md](docs/tech/RUNTIME_EVOLUTION_ROADMAP.md) 头部新增反向引用。
- **Docs:** [API_DESIGN.md](docs/tech/API_DESIGN.md)、[RUNTIME_EVOLUTION_ROADMAP.md](docs/tech/RUNTIME_EVOLUTION_ROADMAP.md) §3 交叉对齐（2026-05-25）— H06 代理认证、IPC ~41 条、模块路径、三文档互链。

### Added

- **Docs:** [docs/desktop/DEV_NOTES.md](docs/desktop/DEV_NOTES.md) §2026-05-24 — product strategy memo (desktop-only shell, TUI/CLI demotion, long-horizon CRAFT ~35 min, industry alignment, D12–D14 candidates, L3 backlog).
- **B2.1:** Injection arbitration SSOT — [docs/tech/adr/B2_INJECTION_ARBITRATION.md](docs/tech/adr/B2_INJECTION_ARBITRATION.md) (tool results > CRAFT blackboard > topic_memory).
- **B-L3:** Zagens `TopicMemoryPanel` + `GET /v1/topic-memory` (graph + eval metrics); settings sidebar entry.
- **B2.5:** `scripts/topic-memory-eval.ps1` — clarification-rate baseline compare + `-Gate`; `TopicMemoryEvalReport` / `compare_eval` in `zagens-topic-memory`.
- **B3.3:** SSE backpressure — `RecvError::Lagged` catch-up from store + `coalesce_delta_events` for `item.delta`.
- **B3.1:** `main.rs` slimmed (~350 lines); CLI tests → `cli/tests.rs`; `cli/entry.rs` (`configure_windows_console_utf8`).
- **B2.3:** k-hop subgraph retrieval (`retrieve_for_query`) seeds injection from the latest user turn instead of pasting the full hot graph.
- **B2.5:** `topic-memory-metrics.json` sidecar (turn updates, inject count, repeat-topic / clarification heuristics).
- **B2 (Zagens):** Settings panel toggles `topic_memory_enabled` and inject interval; persisted to `[topic_memory]` in `config.toml`.
- **B3 CLI:** `run_*` and helpers moved to `crates/tui/src/cli/commands/legacy.rs`; `main.rs` ~1.4k lines (was ~4.9k); `clap` in `cli/args.rs`.
- **Runtime (A1.3):** `RuntimeThreadManager::events_since_async` — HTTP/SSE/task paths offload SQLite/JSONL reads via `spawn_blocking`.
- **Docs:** Backlog ADRs (`BACKLOG_ENGINE_STRUCT_IN_CORE`, `BACKLOG_RUNTIME_UNIFICATION`, `BACKLOG_STATESTORE_JSONL`, `BACKLOG_LANDLOCK_ENFORCE`); `A1_PERSIST_BLOCKING_AUDIT.md`; `tui-core` legacy README.
- **A1:** Live history isomorphism wired at turn complete, tool complete, session load, and backtrack (full `live_history_matches_messages` path). Superseded 2026-05-25 by `App::check_live_history_isomorphism` (production-grade, see §Added above).
- **B2.1:** Capacity guardrail refresh omits `<topic_memory>` via `PromptInjectionArbitration::capacity_pressure()`; regression test in `core/engine/tests.rs`.
- **Tests (CRAFT/GAP):** Unit tests for `instructions_paths` auto-discovery, `resident_file` hard lock, and sub-agent LSP inheritance in `build_tool_context`; G2 §11 smoke runbook for integration sign-off.
- **CRAFT:** Sub-agent `ToolContext` inherits parent `lsp_manager` when LSP enabled — `diagnostics` works in child turns.
- **CRAFT (Issue 6):** `Config::instructions_paths(workspace)` auto-discovers `PROJECT_RULES.md` and `.cursor/rules/*.mdc` when `instructions = [...]` is unset or empty (pick-rules merge unchanged).
- **CRAFT:** `resident_file` hard lock — conflicting lease rejects spawn instead of warning-only.
- **Runtime (GAP):** `POST /v1/threads/{id}/fork-at-user-message` with `{ depth_from_tail }` exposes `fork_at_user_message` for backtrack-depth forking.
- **Zagens (GAP):** `agent_spawn` / `spawn_agent` tool cards show inline sub-agent status linked to AgentPanel SSE state.
- **Zagens (GAP):** User messages (non-last) offer **Branch** → `forkThreadAtUserMessage` + composer prefill from `original_user_text`.
- **Runtime (A1.4):** `history_isomorphism` — live tool-detail outputs vs message tool-results parity helpers + tests.
- **Desktop API:** `forkThreadAtUserMessage()` client helper.

### Fixed

- **Zagens (F3):** `ModelParamsDialog` — `role="dialog"`, `aria-modal`, labelled controls, Escape to close, focus on open; strings via `modelParams.*` i18n.
- **Runtime (A1.4):** `history_isomorphism` — thinking block round-trip + `history_transcript_core_matches_messages`; compaction/trim/persist paths use core check; partition trim regression tests.
- **Zagens (F3):** Right-panel workbench tabs + integrated terminal session tabs use roving `tabIndex` and Arrow/Home/End keyboard navigation (`lib/a11y/rovingTabList.ts`).
- **Runtime (A3.2/A3.4):** `ErrorRetryPolicy` + `user_hint_for_category`; `ErrorEnvelope.hint` and unified HTTP `error` payload (`class`, `retryable`, `retry_policy`, `hint`); TUI status line appends hint; `api_error_payload_includes_taxonomy_fields` regression.
- **Runtime (A+.7):** Register `pending_approvals` before emitting `approval.required` (fixes resolve-approval racing JSONL/SSE); multi-window regression tests `parallel_pending_approvals_resolve_scoped_to_thread_turn` + `sidecar_parallel_pending_approvals_resolve_then_continue`.
- **Runtime (A1.2):** Large-output routing stamps `ToolResult.metadata.large_output` with persisted `meta_path`; `monitor_turn` copies into turn-item `artifact_refs` so JSONL/SQLite items round-trip to `large_outputs/` blobs.
- **Runtime (A3.3):** Stream transparent/outer retries consult `is_stream_failure_retryable` — `InvalidInput` / auth errors no longer burn retry budget; network/timeouts still retry.
- **Runtime (A3):** `classify_error_message` recognizes DeepSeek thinking/reasoning constraint strings as `InvalidInput` (distinct from network disconnect); golden suite centralized in `zagens-core::error_taxonomy`.
- **Desktop (approval):** 系统设置新增 **「自动批准」** 审批策略；非 auto 时 Composer 显示只读「审批：按需审批」等，不再展示无法勾选的复选框；`approval_policy=auto` 时显示可勾选的「自动批准工具调用」。
- **Desktop (F3):** Composer card markup — options/bridge/textarea/actions stay inside `.card` (removed premature close that left input chrome outside the card).
- **Runtime (A+.4):** `sidecar_contract_full_lifecycle` — interrupt endpoint aligned with Zagens (`POST .../interrupt`, not legacy `/stop`).
- **Zagens (F3):** Terminal session tabs — close control no longer nested inside tab button (valid HTML + keyboard activation).

### Changed

- **B2 topic memory:** Default graph path is `~/.deepseek/topic-memory/graph.json` (dedicated folder, not beside `config.toml`); metrics at `~/.deepseek/topic-memory/metrics.json`. Legacy `~/.deepseek/topic-memory.json` is moved on first use.
- **Runtime (A1):** `set_routing_rules` persists via `spawn_blocking` (async HTTP path no longer blocks on JSON I/O).
- **Governance (D10):** 维护者签收解除桌面 Feature freeze（Jason，2026-05-24）— [docs/tech/adr/P2_D10_UNFREEZE_RECORD.md](docs/tech/adr/P2_D10_UNFREEZE_RECORD.md) §4；路线图 §17.4 已勾选。
- **Governance (F3):** G2 手测清单 §8（键盘 a11y 8.1–8.5）维护者签收 ✅（2026-05-24）— [G2_PR5_MANUAL_SMOKE_CHECKLIST.md](docs/tech/adr/G2_PR5_MANUAL_SMOKE_CHECKLIST.md) §6。
- **Governance (§12.4 #2):** **已闭合**（2026-05-24）— Stop / 长跑双壳 / Zagens 审批（G2 §2 + §9）；全量审核 [CODE_REVIEW_2026-05-24.md](deliverables/CODE_REVIEW_2026-05-24.md)。
- **Runtime (P2 PR6a–d):** Turn loop streaming + tool planning/outcomes + `tool_parser` in `zagens-core`; TUI `tool_plans_exec` + split `host_impl/`; `capacity_policy` + `TurnLoopMode` capacity checkpoints; `execute_plan_on_engine` / `detached_execute_with_lock`. Plan: `docs/tech/adr/P2_PR6_TURN_LOOP_L2_MIGRATION_PLAN.md`（PR6 切片已全部落地；ADR/spike 已同步）。
- **Runtime (A1.6 / R-015):** Full baseline @ `8b1538a` — median RSS **29 MB** (3×50 + 1.1 MB fixture, `-Gate` PASS vs 28.5 MB); ADR + `deliverables/runtime-baseline-full-run.log` updated.
- **Runtime (A1-full):** Emergency trim (`trim_oldest_messages_to_budget`) uses hot/cold partition — drops `ColdSummary` first, preserves hot / pinned / `[workshop-ref]` messages; `context_trim::trim_messages_partition_aware`.
- **Desktop (A6.2):** Sandbox settings on non-macOS show explicit **degraded mode** copy (`settings.sandboxDegradedMode`).
- **Runtime (A6.2):** TUI logs `policy_degraded_mode_notice()` once at interactive startup when OS sandbox is degraded.
- **Desktop (F1a):** `TerminalCard` appends `tool.progress` to xterm incrementally instead of full clear+rewrite each frame.
- **Desktop (F1b):** `MessageBubble` shows `DiffCard` while diff tools are still running when unified diff appears in streamed output.
- **Desktop (F3):** Escape stops active generation when focus is outside inputs.
- **Desktop (F3):** Sidebar `role="navigation"`; Composer `#composer-input`, options/actions `role="toolbar"`, DOM tab order (input before options bar; CSS `order-*` keeps visual layout), skip link to composer; send `aria-keyshortcuts="Enter"`.

### Added

- **Runtime (GAP 8a):** `StartTurnRequest` / `StartTurnParams` / session sampling fields (`temperature`, `top_p`, `max_output_tokens`); `streaming_phase` forwards them to the API request.
- **Runtime (GAP F4):** `POST /v1/threads/{id}/edit-last-turn` — truncate last user turn on live engine session and start a new turn (TUI `/edit` parity).
- **Zagens (GAP 8a):** Composer gear opens `ModelParamsDialog`; params persist in localStorage and pass through `startThreadTurn` / `POST /v1/stream`.
- **Zagens (GAP F4):** Edit last user message from `MessageBubble` → dialog → `editLastThreadTurn` + SSE replay.
- **Docs:** 路线图 §17.3 / `IMPLEMENTATION_SUMMARY` 按 2026-05-24 代码审计更新（manager 已拆、F0–F3/路由/导出/托盘/智能粘贴已闭合）。
- **Docs:** G2 §10 B-L1 CRAFT 手测签收（2026-05-24）— §12.5 #1 闭环、AgentPanel、`craft.*` SSE；[G2_PR5_MANUAL_SMOKE_CHECKLIST.md](docs/tech/adr/G2_PR5_MANUAL_SMOKE_CHECKLIST.md) §10。
- **Runtime (B-L1 / CRAFT):** Blackboard APIs bind to thread **workspace** (not sidecar `cwd`); `GET /v1/blackboards` + `GET /v1/blackboards/{id}`; subagent done sentinel includes `structured_verdict` only when present; Verifier failures写入黑板；`<deepseek:craft.fix_loop>` 程序化修复提示；SSE `craft.verdict` / `craft.board_updated`。
- **Zagens (B-L3):** AgentPanel「CRAFT 任务」区域 — 轮询 `/v1/blackboards`，展示 explorer / 实现轮次 / reviewer 裁决 / verifier 摘要。
- **Docs:** `docs/tech/adr/IMPLEMENTATION_SUMMARY_2026-05-24.md` — 路线图门控链与 A/A+/P2/F/D10 实施现状归档；路线图 §17 已链入。
- **Runtime (A1.4):** `tui/history_isomorphism` — user/assistant transcript parity with `history_cells_from_message`; tests after compaction, trim, and JSONL reconstruct.
- **Runtime (A1.1):** `zagens_core::context_partition` — hot window / cold zone tiers (`Hot`, `Pinned`, `ColdSummary`, `ColdExternalRef`); `CompactionPlan::context_partition`.
- **Runtime (A1.2):** Large tool output blobs persist under `~/.deepseek/sessions/<session_id>/large_outputs/` (`persist_large_output_blob`, workshop-ref round-trip test); registry hooks on routed synthesis when `state_namespace` is a session id.
- **Docs (A6.1):** `docs/tech/SANDBOX_CAPABILITY_MATRIX.md` — macOS / Linux / Windows enforcement vs policy-declaration matrix.
- **Runtime (A5.2):** `EngineConfig::llm_client_override` — inject `Arc<dyn LlmClient>` for mock-LLM engine tests (`engine_llm_client_override_runs_mock_turn`).
- **Runtime (A5.3):** Full-engine mock integration tests — compaction, parallel read-only tools, subagent spawn/list, capacity pre-request + mock LLM (`engine_mock_capacity_pre_request_observes_mock_and_emits_decision`).
- **Desktop (F3):** App shell Tab order — Composer before chat in DOM (flex `order` preserves layout); `main`/`aside` landmarks; `role="complementary"` on right panel.
- **Desktop (A+.5):** `runtime_proxy` path allowlist regression tests (`/health`, `/v1/*`; reject traversal and non-v1 prefixes).
- **Runtime (A1.2):** `large_output_tool_item_detail_matches_jsonl_and_persisted_blob` — turn item `detail`, JSONL `item.completed`, and `large_outputs/` blob agree on workshop-ref.
- **Runtime (A1.4):** `reconstruct_messages_matches_jsonl_item_completed_details` — turn-item reconstruct vs JSONL `item.completed` user-visible text isomorphism.
- **Runtime (A1.4):** `compact_messages_safe_preserves_pinned_text_in_result_messages` — compaction pin isomorphism regression.
- **Runtime (P2):** `Op::ApproveToolCall` / `DenyToolCall` route through `tx_approval` (same channel as `EngineHandle`).
- **Desktop (A+.3):** `KNOWN_DESKTOP_SSE_EVENTS` + `streamNormalize.selfcheck.ts` — unknown SSE events return `null`.
- **Desktop (F3):** `npm run test:f3` — roving tablist helper self-check (`rovingTabList.selfcheck.ts`).
- **Runtime (P2):** `SubAgentSpawnPort::list_subagents` — op-loop `ListSubAgents` delegates through port; tui adapter runs manager cleanup + list.
- **Runtime (A2):** `monitor_turn` logs `TurnSummary` on `TurnComplete` with `thread_id` + `turn_id`.
- **Runtime (P2):** `op_handlers.rs` — cancel/approve/deny/list/change-mode/query-context ops; `op_loop` match thinned further.
- **Runtime (P2):** `compaction_ops.rs` / `edit_turn_ops.rs` — manual compaction and `/edit` extracted from `message_handlers`; RLM/compaction delegate via `op_handlers`.
- **Docs:** `A2_TURN_OBSERVABILITY_V1_DRAFT.md` — A2.3 internal + L2 `turn_summary` alignment draft.
- **Runtime (A2):** `TurnSummary::log_turn_complete` — structured `tracing::info!` on engine turn end (aligned with `turn.completed` payload).
- **Runtime (P2):** `handle_spawn_subagent_op`, `apply_set_model_op` / `apply_set_compaction_op` — op-loop delegates; sync status emit folded into `sync_session_from_op`.
- **Runtime (A2 / A2.5):** `turn_streaming` / `turn_tools` `.instrument` spans on streaming + tool phases (`turn_id`/`step`); structured `tracing` events on turn loop + `monitor_turn`.
- **Runtime (P2):** `Engine::engine_context_snapshot` — `Op::QueryContext` delegates through `session_ops.rs`.
- **Runtime (P2):** `session::truncate_before_last_user_message` — `Op::EditLastTurn` via `message_handlers::handle_edit_last_turn`.
- **Runtime (P2):** `zagens-core::session::apply_sync_session_payload` — `Op::SyncSession` via `session_ops::sync_session_from_op`.
- **Runtime (A2):** `zagens-core::events::TurnSummary` — structured `turn_summary` on `turn.completed` (monitor uses core type, not ad-hoc JSON).
- **Runtime (P2):** `zagens-core::session::{is_auto_model_label, apply_model_selection}` — op-loop `SetModel` / `SyncSession` via `session_ops.rs`.
- **Desktop (F3):** Skip links use `:focus-visible` focus ring (keyboard-only, aligned with primary controls).
- **Docs:** `G2_PR5_MANUAL_SMOKE_CHECKLIST.md` §8 — F3 keyboard a11y smoke (Tab / skip link / Escape / focus ring).
- **Runtime (A1.5):** `count_oldest_messages_to_drain` — batch `Vec::drain` instead of repeated `remove(0)` during emergency trim.
- **Runtime (A1-MVP.1):** `LargeOutputExternalRef` + `[workshop-ref: …]` header on routed large tool output.
- **Runtime (A1-MVP.2):** compaction end-to-end test — working-set pinned messages survive LLM summary (`compact_messages_preserves_working_set_pinned_message`).
- **Runtime (A1.3):** runtime thread event append + checklist/scratchpad metadata saves use `spawn_blocking`; crash-safe checkpoint table in `RUNTIME_BASELINE.md`.
- **Runtime (P2):** `lsp_edit_paths` in `zagens-core` — edit-tool path extraction for LSP hooks (tui re-uses core).
- **Runtime (P2):** `SubAgentSpawnPort` in `zagens-core::engine` — op-loop spawn surface; tui `subagent_spawn.rs` adapter.
- **Runtime (PR5):** `RuntimeThreadMessageTurnPort` — sidecar `ThreadMessageTurnPort` adapter over `RuntimeThreadManager::start_turn` + regression test.
- **Runtime (A3.4):** HTTP `ApiError` responses include `category` / `code` / `recoverable` / `severity` from `ErrorEnvelope`.
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
- **Runtime (P2 PR4 局部):** `zagens-core::engine::{dispatch,context}`（工具 JSON/上下文预算/plan 策略）；tui 薄 re-export；`RegistryToolDispatch` 接线 `execute_tool_with_lock`；`Engine`/`turn_loop` 仍留 tui。
- **Runtime (P2 PR4 局部):** `zagens-core::engine::approval`（`await_tool_approval` / `recv_user_input_for_tool`、泛型 `ApprovalDecision<P>`）；tui `approval.rs` 薄壳（`UserInputRequired` 事件仍 L2）；core 加 `tokio`/`tokio-util`。
- **Runtime (P2 PR4 局部):** `zagens-core::engine::{tool_bridge,tool_progress}`（`ToolCall`↔`ToolOutput` 转换、`emit_tool_audit`、进度文案）；tui `tool_dispatch_port` / `tool_execution` 薄壳（`RegistryToolDispatch`、`InteractiveTerminalGuard`、MCP/进度仍 L2）。
- **Runtime (P2 PR4 局部):** `zagens-core::{events,error_taxonomy,coherence,user_input,subagent}` + tui re-export（`Event`/`ErrorEnvelope`/`CoherenceState`/`UserInputRequest`/subagent 类型）；`envelope_from_llm_error` 保留 tui（`LlmError` 孤儿规则）。
- **Runtime (P2 PR4 局部):** `TurnContext`/`TurnLoopMode`/`StreamError` 迁入 `zagens-core`；`TurnLoopHost` + `tool_phase.rs` / `streaming_phase.rs`；**`zagens-core::engine::handle_deepseek_turn`**（generic `TurnLoopHost`）。
- **Runtime (A4.6 局部):** `engine.rs` 拆出 `types.rs`（`EngineConfig`）、`handle.rs`、`engine_new.rs`、`engine_helpers.rs`、`session_messages.rs`、`mock.rs`；`engine.rs` ~618 → **~201 行**（达 PR4 spike **< 300** 目标）。
- **Runtime (A4.6 局部):** `engine.rs` 拆出 `op_loop.rs`、`cycle_hooks.rs`、`message_handlers.rs`（`handle_send_message` / 手动 compaction）；`engine.rs` ~2177 → ~1220 行。
- **Runtime (P2 PR4 局部):** `TurnLoopToolExecutor` + `TurnLoopToolRegistry` 关联类型；`Engine` / `McpPoolHandle` 端口实现。
- **Runtime (tests):** A5.5 最小回放 fixture `tests/fixtures/runtime_turn_minimal.jsonl` + 顺序/seq 断言。
- **Runtime (P2 PR4 局部):** `zagens-core::engine::tool_catalog`（deferral、tool search、missing-tool 文案）；tui 薄壳保留 `AppMode` 适配与 `code_execution` 子进程。
- **Docs:** `docs/tech/adr/P2_DESKTOP_TURNLOOP_SPIKE.md` — Zagens 经 sidecar HTTP 使用 `TurnLoopHost`（tui `host_impl`），desktop crate 不链接 `Engine`。
- **Runtime (A4.6 局部):** `engine/capacity_flow/{checkpoints,observation,events,interventions,replay,persistence}.rs`；原 monolith ~985 行拆为 6 个子模块（最大 ~370 行）。
- **Runtime (A4.6 局部):** `runtime_threads/turn_control.rs`（`interrupt_turn` / `steer_turn` / `compact_thread`）；`manager.rs` ~829 → ~589 行。
- **Runtime (A4.6 局部):** `runtime_threads/thread_crud.rs`（create/list/get/update/fork/resume/seed 等）；`manager.rs` ~1673 → ~1032 行。
- **Runtime (A4.6):** `runtime_threads/engine_load.rs`（`ensure_engine_loaded`）；`routing.rs` 路由读写。
- **Runtime (tests):** `runtime_api` 测试显式隔离 `data_dir`，不再受工作区 `DEEPSEEK_RUNTIME_DIR` 污染。
- **Runtime (A4.6):** `runtime_threads/routing.rs` 自 `manager.rs` 拆出路由规则读写。
- **Runtime (A4.6):** `runtime_threads/{active,monitor}.rs` 自 `manager.rs` 拆出（LRU/活跃 turn 状态 + `monitor_turn`）；`manager.rs` ~1.8k 行。
- **Runtime (P2 PR3 局部):** `zagens-core::engine::{StartTurnParams, TurnEnginePort}`；`RuntimeThreadManager::start_turn` 经 core 委托 `EngineHandle`（`turn_loop` 仍在 tui）。
- **Docs:** [RUNTIME_EVOLUTION_ROADMAP.md](docs/tech/RUNTIME_EVOLUTION_ROADMAP.md) **v2.0-final** — 维护者签收 §4.2（D4–D7、D9）；§17 实施后审核（2026-05-22）；[adr/RUNTIME_BASELINE.md](docs/tech/adr/RUNTIME_BASELINE.md) R-015 占位（基准填数并行）。
- **Runtime (P2 PR1 局部):** Shared types and `LlmClient` trait in `zagens-core` (`chat`, `models`, `turn`, `compaction`, `capacity`, `workshop`, …) with `deepseek-tui` re-exports; `deepseek-tui` **lib** target (`crates/tui/src/lib.rs`).

### Changed

- **Runtime (R-003 / A4.6 阶段 3):** Extract `runtime_threads/tests.rs`；`mod.rs` ~275 行（契约测外置）。
- **Runtime (P2 PR2 局部):** Move `Session`/`SessionUsage`、`working_set`、`project_context`、`ApprovalMode`、`CycleBriefing` into `zagens-core` with tui re-exports; `turn_loop` still in `deepseek-tui::core::engine`.
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

- **Runtime (RLM):** `RlmLlmClient` blanket impl uses `?Sized` so `Arc<dyn LlmClient>` compiles after `LlmClient` moved to `zagens-core`.
- **Tests:** `cargo test -p deepseek-tui --lib` green (2368 passed) — JSON-only fixtures for schema-rejection tests (SQLite migration), `read_file` metadata key `total_lines`, Windows `pwsh` shell/display_command, approval resolve `tokio::join!` + stale-turn immediate deny, mock-engine turn timeout 8s for `QueryContext` panel emit.
- **Tests:** `subagent` stub runtime wraps client in `Arc` for P2 client type.

### Changed

- **Runtime (R-003 / A4.1):** Extract `runtime_api/router.rs` (`build_router`); handlers remain in `mod.rs` for now.

## [0.4.3] - 2026-05-21

### Zagens (desktop)

- **v0.4.3** — `zagens-desktop`、`tauri.conf.json`、`web-ui/package.json` 与 About 面板对齐 **v0.4.3**。

### Fixed

- **Zagens (desktop):** Fix multi-window / continued-session chat stream duplication (`看到了看到了` / `TheThe user`) — runtime SSE proxy uses `emit_to` per window; Web UI binds SSE via `getCurrentWebviewWindow().listen`; resumed turns poll `replay_only` events instead of a long-lived `GET …/events` SSE (avoids stacked `runtime_get_sse` streams); `runtime_cancel_sse` stops in-flight proxy reads on abort; `finishOnce` aborts the turn `AbortSignal` after `turn.completed`.

## [0.4.2] - 2026-05-21

### Zagens (desktop)

- **v0.4.2** — `zagens-desktop`、`tauri.conf.json`、`web-ui/package.json` 与 About 面板对齐 **v0.4.2**。

### Added

- **Zagens (desktop):** True multi-window (Cursor / VS Code model) — `WebviewWindow` per project, `tauri-plugin-single-instance`, tray/menu **新建窗口**, TitleBar + **Ctrl/Cmd+Shift+N**; per-window workspace `localStorage`, session list filter + **显示全部会话**; parallel turns per `thread_id` (switch session no longer aborts other streams); terminal `emit_to` per window; approval routed via `register_window_thread` / `thread_owned_by_window`.
- **Docs:** [multi-window-plan.md](docs/desktop/multi-window-plan.md) — multi-window plan **closed** (M1–M4 shipped; M5 deferred to backlog §7.5).

## [0.4.1] - 2026-05-21

### Zagens (desktop)

- **v0.4.1** — `zagens-desktop`、`tauri.conf.json`、`web-ui/package.json` 与 About 面板对齐 **v0.4.1**。

### Added

- **Docs:** [workspace-directory-plan.md](docs/desktop/workspace-directory-plan.md) — workbench Directory tab phased UI/feature plan with implementation checklist (§0, §10).
- **Zagens (web UI):** Workbench Directory tab — flat stroke icons, toolbar (up/refresh/open folder), search filter, hidden-folder toggle (`target`, `node_modules`, etc.), merged workspace path row, scrollable list, preview highlight, `WorkspaceFilesPanel` + i18n `workspaceFiles.*`.
- **Zagens (web UI):** Workbench Directory **tree view** (phase D) — lazy-loaded `WorkspaceFileTree`, per-workspace expanded-state in `sessionStorage`, list/tree toggle with flat stroke icons.
- **Zagens (web UI):** i18n for workbench panel — `panels.*`, `workbench.*`, `workspaceFiles.tab` / `workspaceFiles.errors.*`; `RightPanel` and workspace file open errors use `useT`.
- **Zagens (web UI):** Tasks panel — **Clear finished** removes completed / failed / canceled records via runtime `POST /v1/tasks/clear` (queued and running tasks kept); confirmation dialog + toast.
- **Zagens (web UI):** Workbench Files tab — workspace-wide file search (same input box, BFS via browse API; skips denylisted dirs), virtual list for large flat/search result sets (B4); chat/Diff「在目录中显示」, scroll-to-reveal, keyboard shortcuts, Office directory presets.
- **Zagens (Phase D1):** Audit scratchpad — expandable inventory list from `scratchpad/status` `areas[]`, U1 contract violation highlight (notes without accounted areas), i18n strings; path click opens workspace preview.
- **Zagens (web UI):** Audit scratchpad colors aligned with app theme (`bg-card`, `text-t-text`, accent/error tokens) — readable in light mode; inventory status chips match ToolCard-style badges.
- **Zagens (Phase D2):** Scratchpad status API — `checklist_completed/total`, `contract_warnings`, findings severity tallies; audit panel dual-track (inventory vs checklist), findings strip, sub-agent active count + narrative-spawn warning; checklist tool events refresh panel.
- **Zagens (Phase D U2):** Sidebar separates **Tasks** (`GET /v1/tasks`) and **Sub-agents** (`agent_*` SSE) into top-level inspector entries; **Skills** stays under Settings. Checklist / Tasks / Sub-agents show a **small activity dot** when there is in-flight work or unseen updates (pulse while running); opening the panel clears the indicator.
- **Zagens (web UI):** **Usage** dashboard moved to the same sidebar tier as Checklist / Tasks / Sub-agents (display panels); Settings keeps API Key, MCP, Skills, routing, index, and system config only.
- **Zagens (web UI):** Audit scratchpad moved from the chat composer strip to a sidebar **审计** entry and right **Audit scratchpad** panel (same behavior as Checklist); sidebar activity dot when a run is active.
- **Zagens (web UI):** Chat **Reasoning** and **tools** sections get clipboard copy (section header + per-tool card); uses shared `copyPlainText` helper.
- **Zagens (web UI):** Sub-agent panel shows spawn **objective**, type/role, work-package id, and progress line; runtime `agent.spawned` SSE includes `prompt`; bogus `call_*` tool-call ids are no longer listed as agents.
- **Zagens (panel channel C):** Runtime emits `panel.scratchpad` / `panel.checklist` / `panel.context` on the live SSE stream; Web UI applies them directly and uses slow B-channel polls only as fallback while streaming.

### Changed

- **Zagens (web UI):** Audit scratchpad panel — uniform card border on all sides (removed thick left accent stripe); attention state uses a subtle amber border tint instead.
- **Zagens (web UI):** Remove redundant「打开文件夹」button from workbench panel header; use「在文件管理器中打开」in the Files tab toolbar instead.

### Fixed

- **Zagens (web UI):** Workbench Files「添加至对话」/「Add to chat」 now inserts an `@` workspace path into Composer instead of opening the preview panel.
- **TUI / runtime (security, L7d P1/P2):** Session `/load` `/save` `/export` paths resolved under workspace (`path_guard`); MCP no longer trusts client `approved` for shell/write tools when `require_approval` is on; default MCP expose list is read-only (`file_read`, `search`, `file_search`); cancel token reset order fixed (H01); `Config` Debug redacts API keys; file-picker labels strip control chars; scratchpad coverage preview uses char-safe truncation; Linux/Windows sandbox types surface an explicit unenforced warning on `exec_shell` (H12); Python REPL spawns with `-I` and docs state no OS isolation (H13).
- **Zagens (security, L7d follow-up):** P0 from `2026-05-20-001` audit — `export_*_json` validates `.json` path (no `..`/system dirs); runtime Bearer no longer exposed via `get_runtime_token` (Tauri `runtime_http` / `runtime_post_stream` / `runtime_get_sse`); Explore sub-agent `explicit_tools` intersected with read-only cap; blackboard `task_id` restricted to safe charset.
- **Zagens (web UI):** Mermaid SVG, diff2html output, and clipboard HTML paste sanitized with DOMPurify before `innerHTML`.
- **Zagens (web UI):** Long-audit HTTP poll storm — coalesced in-flight GETs for context/checklist/scratchpad status; longer staggered intervals while streaming; session checkpoint 60s (turn-complete + tab-hide still persist); runtime probe 18s during stream with immediate light `/health` on stream start.
- **Zagens (runtime):** `scratchpad/status` and `checklist` handlers run on the blocking pool (2s status cache) so `/health` and SSE stay responsive under audit load.
- **Zagens (web UI):** Sidebar「未连接」during long audits while generation still runs — periodic probe no longer requires `/v1/sessions` (2.5s timeout) while streaming; uses `/health` only so busy sidecar is not misread as offline.
- **Zagens (web UI):** Right panel (workspace browse, MCP, tasks/skills, routing, usage) no longer hard-blocks on probe `offline` during streaming; session-list refresh failures use light probe; sidebar shows amber「繁忙（生成中）」when degraded.
- **Zagens (web UI):** `runtimeSessionEstablished` keeps checklist, workspace, audit bar, and MCP panels on API paths after connect/resume — probe blips no longer gate panel fetches; probe requires 3 consecutive failures before `offline`; poll GETs use 45s timeout and retain last checklist/scratchpad snapshot on busy errors.

## [0.4.0] - 2026-05-20

### Zagens (desktop)

- **v0.4.0** — `zagens-desktop`、`tauri.conf.json`、`web-ui/package.json` 与 About 面板对齐 **v0.4.0**。

### Added

- **Zagens (web UI):** Runtime-aligned context usage via `GET /v1/threads/{id}/context` (TUI `estimate_input_tokens_conservative` + compaction policy); Composer shows runtime estimate when sidecar is connected.
- **Zagens (web UI):** Fix context usage indicator resetting to 0% after switching sessions and back (per-thread snapshot cache, stale refresh guard, transcript fallback when runtime snapshot is empty).
- **Zagens (web UI):** Dual-track context display — progress ring uses conservative estimate; Composer also shows last API `input_tokens` from the provider when available.
- **TUI / runtime:** Engine records per-round API `input_tokens` (`last_api_input_tokens`); context snapshot exposes `last_api_usage_percent`; token estimate uses DeepSeek doc ratios (CJK ~0.6, ASCII ~0.3 per char).
- **Zagens (system settings):** `[compaction]` — `auto_compact` toggle and `token_threshold` (synced to `config.toml`, shared with TUI engine compaction).
- **Audit scratchpad (Phase B):** Runtime tools `scratchpad_*`; `ScratchpadStore` + layered P2 summary injection, readonly nudge (B4), cycle handoff pointer (B3b), `ThreadRecord.scratchpad_run_id` (B2), TTL cleanup (B7), `GET /v1/threads/{id}/scratchpad/status`, Zagens `AuditScratchpadBar` (B5). Config: `[scratchpad]` in `config.toml`.
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
- **Docs:** [HARNESS.md](docs/desktop/HARNESS.md) — Agent Harness 定位（社招 JD 映射、Zagens 栈位、会话恢复案例、与 DeepSeek 关系备忘 §7）。
- **Docs:** [audit-scratchpad-design.md §6.13](docs/desktop/audit-scratchpad-design.md) — Phase D 审计过程可视化路线图（D1–D3、产品/模型边界）；[audit-scratchpad-test.md §L7c/L8/L9](docs/desktop/audit-scratchpad-test.md) — `2026-05-20-audit` 试跑记录、可视化验收、地狱级四维暂缓。

### Fixed

- **Zagens (web UI):** Saving system settings restarts the sidecar — UI no longer stays on「生成中」while the sidebar shows「未连接」; `sidecar://restarting` clears the stream; confirm dialog when saving during an active turn.
- **Zagens (web UI):** Stop assistant reply body **jitter while streaming** — plain pre-wrap during tokens (Markdown after turn completes), single scroll owner (no outer 200ms poll vs inner 48vh cap), fixed-height「生成中」footer.
- **Zagens (desktop):** Content-Security-Policy — add `font-src` (`'self'`, `data:`, dev localhost / `tauri.localhost`) so bundled UI fonts are not blocked (console CSP violation on `data:font/woff2`).

### Changed

- **Zagens (web UI):** Audit scratchpad bar — dismiss control (×, top-right); hidden for the same thread/run until a new scratchpad run (sessionStorage).
- **Zagens (web UI):** Audit scratchpad bar — neutral `canvas-alt` shell (aligned with tool cards); contract reminders use amber pill + left rail instead of full error red styling.
- **Zagens (web UI):** Context ring prefers runtime snapshot over client transcript estimate; polls during streaming.
- **Audit scratchpad (L7b short-term):** Expand `[scratchpad] inject_on_report_keywords` (E1); block `write_file` to `deliverables/` audit/CODE_REVIEW paths when bound scratchpad inventory incomplete or C1 hard gate fails (E2, `scratchpad_flow::check_write_file_audit_report_gate`); **E5** — during bound audit scratchpad defer/block `task_create` and eager-load `agent_spawn` (+ join tools) so P1 parallel review uses sub-agents not TaskManager.
- **Zagens / config:** `[subagents] step_timeout_secs` — configurable default per-step sub-agent LLM API timeout (10–600 s); system settings slider; `agent_spawn` uses it when `step_timeout_ms` is omitted (replaces hard-coded 120 s default).
- **Sub-agents / prompts:** Step API timeout errors and `base.md` / `audit-repo` spell out that omitted `step_timeout_ms` is not unlimited time; parents must re-spawn or shrink scope on timeout — not mark audit areas done.
- **Prompts / tools:** Clarify **Task** (`task_*`, peer durable work) vs **Sub-agent** (`agent_*`, parent-dispatched) in `base.md`, `tasks.rs` tool descriptions, and `agent_spawn` / `agent_result` / `agent_list` descriptions; `task_id` spawn param documented as blackboard key only.
- **Zagens (web UI):** Custom context menu on chat workspace file links — open with system app, copy absolute path, copy relative path; suppresses the native WebView link menu (e.g. non-functional “open in new window” on `href="#"`).
- **Zagens (web UI):** Right workbench panel **collapsed by default** on launch (left sidebar stays open); use the edge strip to expand.
- **Zagens (web UI):** Composer **Stop** calls `POST …/turns/{turn_id}/interrupt` (runtime `engine.cancel()`), not only aborting the SSE client—matches TUI Ctrl+C / Esc interrupt semantics.
- **Audit scratchpad (discoverability):** `scratchpad_*` tools eager-loaded in Agent (not deferred); `scratchpad_status` / `scratchpad_list_notes` bind `thread.scratchpad_run_id`; `GET …/scratchpad/status` auto-discovers latest `inventory.json` when unbound (Zagens bar). `audit-repo` skill + `base.md` / pick-rules §7: `tool_search` before `write_file` fallback.
- **Zagens (web UI):** Audit bar shows **accounted** progress (done + in_progress + deferred), faster poll while streaming, refresh on scratchpad tool completion. **Sub-agent panel:** forward `agent.spawned` / `agent.progress` / `agent.completed` / `agent.list` on compat SSE (`POST /v1/stream`).
- **Zagens (web UI):** Dark theme — user message text uses dedicated high-contrast `--color-msg-user-text` (fixes faint prose grays in user bubbles).
- **Zagens (web UI):** While an assistant reply is **streaming**, the main body uses the same **48vh scroll cap** as Reasoning so CoT stays on screen; after the turn completes, the body **ease-out expands** to full height (respects `prefers-reduced-motion`).
- **Zagens (web UI):** Sidebar / right-panel **collapse** controls move to the resize gutter—hidden until hover on the `col-resize` seam; panel-indent icon replaces header chevrons.
- **Zagens (web UI):** Global **toast** notifications (no new npm deps) replace the chat-column amber **banner**; stack centered above the composer with success / error / warning / info variants; runtime reachability errors include **Retry connection** and auto-dismiss when the sidecar probe is healthy.
- **Zagens (web UI):** Assistant **Reasoning** and **工具调用** blocks default to **collapsed**; click header to expand (streaming shows “推理中…” / “N 个进行中” hints while folded).
- **Docs:** [README.md](README.md) — lead with verified differentiators; split desktop vs shared runtime; trim misleading feature tables; fix dev commands (`cargo tauri dev`); align doc links (`API_DESIGN.md`, `DEV_NOTES.md`). Cursor/portable rules updated for dead links.

## [0.3.0] - 2026-05-19

### Zagens (desktop)

- **v0.3.0** — `zagens-desktop`、`tauri.conf.json`、`web-ui/package.json` 与侧栏标签对齐 **v0.3.0**。
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

- **Zagens** — Light theme “极淡乳白” palette (warm stone canvas/card, softer dividers, blue accent retained); center chat column uses `card` surface.
- **Zagens** — Dark theme “深色暖 · 护眼” palette (warm gray-black shell, amber accent, softer status colors).
- **Zagens** — Sidebar brand row: flat logo + accent “Zagens” (no gray pill), aligned with reference mockups.
- **Zagens** — Bundled UI fonts: Plus Jakarta Sans Variable (Latin) + Noto Sans SC (CJK); JetBrains Mono for terminal/code; replaces system Segoe/Roboto stack.
- **Zagens** — Sidebar: app icon in brand row; title bar brand text removed; connection status at bottom (“连接正常” / “未连接”); version blurb moved to **关于** panel under Settings.
- **Runtime** — 消除 tokio worker 中的阻塞 I/O；诊断与相关文档更新。

### Fixed

- **Zagens** — After app restart, **Reasoning** and **tool** cards restore correctly: `resume-thread` reuses persisted `runtime_thread_id` for event replay (instead of seeding a blank thread); `persist-session` stores that id; web UI mirrors UI snapshots to `localStorage` as fallback.
- **Zagens** — Session restore `GET …/events?replay_only=1` no longer returns HTTP 400 (accept `1`/`0` query booleans; client uses `replay_only=true`).
- **Zagens** — Switching sessions keeps in-memory UI snapshots (tools + thinking); thread event replay still refreshes from runtime when available.
- **Zagens** — Checklist panel: sidebar **清单** entry, persist `checklist_write` on thread record (survives sidecar restart), faster poll while streaming; auto-switch no longer blocks manual **工作台** tab during streaming.
- **Zagens** — Left sidebar width is draggable on the column edge (persisted; 180px–45% viewport); fixes broken resize handle that was absolutely positioned without a containing block.
- **Zagens** — Unified divider tokens (`chrome-seam` vs `divider` vs `card-border`); column resize gutters use inset seams only; shell panels use tint bands instead of stacked border lines.
- **Desktop** — Unicode 工作区路径下的可点击聊天链接。
- **xlsx** — 生成路径六项生产级修复。

### Security

- CODE_REVIEW **Step 1** — 依赖与安全清理（见 `docs` 中 Step 1 跟踪）。

### Docs

- Agent 可靠性/CRAFT/符号索引/V5 等规划与实施记录更新；部分旧版 brief 归档或移除。

---

## [0.2.2] - 2026-05-11

### Zagens (desktop)

- **v0.2.2** — `zagens-desktop`、Tauri `version`、`web-ui/package.json` 与侧栏标签对齐 **v0.2.2**；打包脚本与 bundled Python 准备（`prepare-python.mjs`、`docs/bundled-python-plan.md`）。
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

### Zagens (desktop)

- **v0.2.1** — 独立桌面 SemVer；侧栏 **Zagens v0.2.1**（workspace crate 依赖仍为共享线，如 `0.8.15`）。
- **三栏 shell 增强** — 统一聊天壳、右侧面板（runtime API 对接）、devtools 开关、窗口权限与标题栏。
- **Markdown** — 聊天区 Markdown 渲染；`` `path/to/file` `` 与相对 `[](…)` 链接在工作台打开预览；预览区内相对链接 **应用内** 打开（外链新窗口，`#` 锚点滚动）。
- **启动** — Zagens 与 sidecar 就绪速度优化。

### Docs


---

## [0.2.0] - 2026-05-09

### Zagens (desktop)

- **v0.2.0** — Zagens 产品化里程碑：文档布局、Cursor 规则、桌面与 runtime 一批修复。
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

- README Zagens 章节与仓库布局表；桌面文档迁入 `docs/desktop/`。

---

## [0.1.0] - 2026-05-07

### Added

- **Runtime API** — `/v1/...` 契约与 [docs/RUNTIME_API.md](docs/RUNTIME_API.md) 实施文档（Phase 1）。

### Changed

- Desktop sidecar / main Rust 源码 `rustfmt`。
