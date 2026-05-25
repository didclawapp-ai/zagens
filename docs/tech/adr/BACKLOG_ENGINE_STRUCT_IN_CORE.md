# Backlog ADR — Move `Engine` struct into `deepseek-core`

**Status:** **In spike (M-series, 2026-05-25)** → see [PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md](./PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md)  
**Related:** [P2_G3_ENGINE_L2_SIGNOFF.md](./P2_G3_ENGINE_L2_SIGNOFF.md), [PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md](./PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md)

## Context

P2 moved `turn_loop` phases into `deepseek-core` with `TurnLoopHost`. The `Engine` struct (MCP, LSP, subagents, channels) remains in `crates/tui`.

## Decision (draft)

Defer whole-struct migration until tool/MCP boundaries are trait-stable. Prefer incremental port of **session op queue** types, not a monolithic move.

**M-series strangler plan adopted 2026-05-25** — see [PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md](./PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md) for the dependency graph, 35-field ownership table, 12 risks, and 7-PR sequence (M1 → M8). Tools / MCP / LSP / sandbox / seam / cycle stay in tui as trait-bridged hosts.

## Acceptance (when undertaken)

- `crates/tui/src/core/engine.rs` is re-export/wiring only (target ≤ 80 LOC, see spike §6 M7).
- Zagens sidecar behavior unchanged (contract tests green).
- Spike §7.4 acceptance checklist (M1) + each M-PR's §6 regression block green.
