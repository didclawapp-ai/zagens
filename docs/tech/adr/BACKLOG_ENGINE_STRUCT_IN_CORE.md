# Backlog ADR — Move `Engine` struct into `deepseek-core`

**Status:** **Closed (M1–M8 landed, 2026-05-26)** — see [PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md](./PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md)  
**Related:** [P2_G3_ENGINE_L2_SIGNOFF.md](./P2_G3_ENGINE_L2_SIGNOFF.md), [PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md](./PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md)

## Context

P2 moved `turn_loop` phases into `deepseek-core` with `TurnLoopHost`. The `Engine` struct (MCP, LSP, subagents, channels) remained in `crates/tui` until the M-series strangler (2026-05-25 → 2026-05-26).

## Decision

**Completed.** Incremental M1–M8 PR sequence moved session op types, config split, host traits, capacity/coherence, the `Engine` struct + builder, and the op loop into `deepseek-core`. Tui retains a thin `#[repr(transparent)]` newtype wrapper for inherent impls and `TurnLoopHost`.

## Progress (final)

| PR | Title | Status | Notes |
|----|-------|--------|-------|
| **M0** | Spike | ✅ | Maintainer-approved structure. |
| **M1** | `Op` + `EngineHandle` + `ThreadContextSnapshot` | ✅ | |
| **M2** | `EngineConfig` split | ✅ | |
| **M3** | Lsp / SubAgent / Shell / Sandbox hosts | ✅ | |
| **M4** | `McpHost` | ✅ | |
| **M5** | Seam / Workshop / TopicMemory + scratchpad state | ✅ | |
| **M6** | `CapacityController` + coherence reducer | ✅ | |
| **M7** | `Engine` struct + `with_hosts` + tui builder | ✅ 2026-05-26 | Core `runtime.rs` / `host_bundle.rs` / `runtime_new.rs`; tui newtype + `build.rs` / `runtime_ext.rs`. |
| **M8** | `op_loop` + cleanup | ✅ 2026-05-26 | Core `op_loop.rs` + `EnginePlatformExt`; tui `platform_dispatch.rs`; BACKLOG closed; handoff deleted. |

## Acceptance (met)

- Core owns `Engine` struct, channels, and op loop; tui `engine.rs` is wiring + newtype (~130 LOC with module tree).
- Sidecar contract tests green (`runtime_api::tests::sidecar_contract_full_lifecycle`).
- Spike §6 regression block green for M7/M8 land (incl. former pre-existing topic_memory + capacity engine tests).

## Follow-ups (non-blocking)

- Optional `crates/runtime-server` sidecar binary without ratatui (see ARCHITECTURE_ASSESSMENT D6).
- Move remaining `capacity_flow/*` / turn helpers opportunistically if sidecar extraction needs them.
