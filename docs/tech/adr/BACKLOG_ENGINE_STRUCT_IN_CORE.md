# Backlog ADR — Move `Engine` struct into `deepseek-core`

**Status:** **In progress (M1 + M2 + M3 landed, 2026-05-25)** → see [PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md](./PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md)  
**Related:** [P2_G3_ENGINE_L2_SIGNOFF.md](./P2_G3_ENGINE_L2_SIGNOFF.md), [PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md](./PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md)

## Context

P2 moved `turn_loop` phases into `deepseek-core` with `TurnLoopHost`. The `Engine` struct (MCP, LSP, subagents, channels) remains in `crates/tui`.

## Decision (draft)

Defer whole-struct migration until tool/MCP boundaries are trait-stable. Prefer incremental port of **session op queue** types, not a monolithic move.

**M-series strangler plan adopted 2026-05-25** — see [PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md](./PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md) for the dependency graph, 35-field ownership table, 12 risks, and 7-PR sequence (M1 → M8). Tools / MCP / LSP / sandbox / seam / cycle stay in tui as trait-bridged hosts.

## Progress

| PR | Title | Status | Net LOC | Notes |
|----|-------|--------|---------|-------|
| **M0** | Spike (this doc + parent) | ✅ landed 2026-05-25 | 0 | Maintainer-approved structure. |
| **M1** | `Op` + `EngineHandle` + `ThreadContextSnapshot` to core | ✅ landed 2026-05-25 | +99 net | `coherence.rs` reducer pushed to M6 (depends on `core::capacity` types that are tui-only until M6). `impl TurnEnginePort` moved core-side (orphan rule). `TurnLoopMode::from_setting` added. tui shims: `core/ops.rs`, `core/engine/handle.rs`, `context_snapshot.rs`. All §7.4 regression tests + `sidecar_contract_full_lifecycle` + web-ui `test:f3` + `npm run build` green. |
| **M2** | `EngineConfig` split (type pillars) | ✅ landed 2026-05-25 | +378 net | core lean `EngineConfig` (25 fields) + tui `EngineConfigExt` (8 fields) established. tui `EngineConfig` keeps flat 30-field layout as a facade — callers untouched. Accessors `lean()` / `ext()` / `into_parts()` / `from_parts()` carve the projection. 2 round-trip unit tests + spike §6 M2 regression suite (`engine_llm_client_override_runs_mock_turn` + 36 core error_taxonomy + `sidecar_contract_full_lifecycle` + `npm run test:f3`) green. `Engine::new(slim, ext)` two-arg switch deferred to M7 (avoids rewriting ≈30 literal construction sites mid-strangler). |
| **M3** | Subsystem traits (Lsp/SubAgent/Shell/Sandbox) | ✅ landed 2026-05-25 | ~+320 net | Engine boundary traits established in `deepseek_core::engine::hosts::{LspHost, SubAgentHost, ShellHost, SandboxHost}` — strictly **call-graph driven** (spike §5 R1): `LspHost` 2 methods (`enabled` + `diagnostics_for`), `SubAgentHost` 3 methods (`spawn_general` + `list_with_cleanup` + `running_count`), `ShellHost` empty marker (Engine never invokes shell methods directly), `SandboxHost` single `backend()` accessor. Data types moved into core: `lsp::diagnostics` (Diagnostic/DiagnosticBlock/Severity/render_blocks) and `sandbox` top-level (SandboxBackend trait + SandboxOutput + SandboxKind). Tui keeps re-export shims under `crate::lsp::*` and `crate::sandbox::backend::*` so existing imports (28k tools, engine tests) compile unchanged. `SubAgentSpawnPort` deprecated alias kept for 1 cycle. Inline `impl ...Host` on existing tui types (`LspManager`, `Engine`) + two newtype wrappers (`TuiSandboxHost`, `TuiShellHost` — the latter because bare `ShellManager` is `Send` but not `Sync`). Engine call sites in `lsp_hooks.rs` + `no_tool_uses.rs` rewired to use the traits. Acceptance per spike §6 M3: `core --lib lsp/sandbox` 11+2 ok, `tui --lib tools::subagent` 108/108 ok, full §6 regression block + `sidecar_contract_full_lifecycle` ok, `cargo build -p deepseek-{core,tui}` clean. |
| **M4** | `McpHost` trait | ⏳ queued | ≤500 cap | M3 establishes the host-trait pattern; M4 follows the same shape for `crates/tui/src/mcp.rs` (2218 LOC). |
| **M5** | Seam / Cycle / Workshop / TopicMemory hosts + scratchpad state | ⏳ queued | ≤700 cap | |
| **M6** | `CapacityController` → core + coherence reducer | ⏳ queued | ≤700 cap | Picks up M1's deferred `coherence.rs` work. |
| **M7** | `Engine` struct + `engine_new` + `op_handlers` into core | ⏳ queued | ≤700 cap | |
| **M8** | `op_loop` into core + final cleanup | ⏳ queued | ≤700 cap | Closes this ADR. |

## Acceptance (when undertaken)

- `crates/tui/src/core/engine.rs` is re-export/wiring only (target ≤ 80 LOC, see spike §6 M7).
- Zagens sidecar behavior unchanged (contract tests green).
- Spike §7.4 acceptance checklist (M1) + each M-PR's §6 regression block green.
