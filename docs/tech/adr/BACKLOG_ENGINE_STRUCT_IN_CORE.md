# Backlog ADR — Move `Engine` struct into `deepseek-core`

**Status:** **In progress (M1 landed, 2026-05-25)** → see [PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md](./PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md)  
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
| **M2** | `EngineConfig` split | ⏳ queued | ≤700 cap | Will split lean core `EngineConfig` from tui `EngineConfigExt`. |
| **M3** | Subsystem traits (Lsp/SubAgent/Shell/Sandbox) | ⏳ queued | ≤700 cap | |
| **M4** | `McpHost` trait | ⏳ queued | ≤500 cap | |
| **M5** | Seam / Cycle / Workshop / TopicMemory hosts + scratchpad state | ⏳ queued | ≤700 cap | |
| **M6** | `CapacityController` → core + coherence reducer | ⏳ queued | ≤700 cap | Picks up M1's deferred `coherence.rs` work. |
| **M7** | `Engine` struct + `engine_new` + `op_handlers` into core | ⏳ queued | ≤700 cap | |
| **M8** | `op_loop` into core + final cleanup | ⏳ queued | ≤700 cap | Closes this ADR. |

## Acceptance (when undertaken)

- `crates/tui/src/core/engine.rs` is re-export/wiring only (target ≤ 80 LOC, see spike §6 M7).
- Zagens sidecar behavior unchanged (contract tests green).
- Spike §7.4 acceptance checklist (M1) + each M-PR's §6 regression block green.
