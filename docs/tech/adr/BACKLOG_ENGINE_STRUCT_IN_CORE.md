# Backlog ADR — Move `Engine` struct into `deepseek-core`

**Status:** Proposed (not scheduled)  
**Related:** [P2_G3_ENGINE_L2_SIGNOFF.md](./P2_G3_ENGINE_L2_SIGNOFF.md)

## Context

P2 moved `turn_loop` phases into `deepseek-core` with `TurnLoopHost`. The `Engine` struct (MCP, LSP, subagents, channels) remains in `crates/tui`.

## Decision (draft)

Defer whole-struct migration until tool/MCP boundaries are trait-stable. Prefer incremental port of **session op queue** types, not a monolithic move.

## Acceptance (when undertaken)

- `crates/tui/src/core/engine.rs` is re-export/wiring only.
- DS Pick sidecar behavior unchanged (contract tests green).
