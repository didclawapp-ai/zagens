# D6 — `deepseek-runtime` sidecar (runtime-server crate)

**Status:** Landed (phase A + A+ + **phase B** — 2026-05-26)  
**Related:** [ARCHITECTURE_ASSESSMENT_2026-05-25.md](./ARCHITECTURE_ASSESSMENT_2026-05-25.md) §1 #5 · **[详细实施方案](./D6_IMPLEMENTATION_PLAN.md)** · **[Phase B ADR](./D6_PHASE_B_CLI_SUNSET.md)**

## Context

Zagens desktop embeds a local HTTP sidecar. Before D6, the bundled binary was `deepseek-tui serve --http`, linking **ratatui**, crossterm, and the full CLI/TUI surface — contradicting the Desktop-only / sidecar-first architecture.

M-series (D5) moved `Engine` + op loop into `deepseek-core`, removing the structural blocker for a headless runtime binary.

**Phase B (2026-05-26)** merged the runtime lib into a **single crate** (`crates/runtime-server`, lib `deepseek_runtime`) and **removed** `crates/cli`, `crates/tui`, and the ratatui TUI tree.

## Decision

### Phase A (2026-05-26)

1. Add workspace crate **`deepseek-runtime-server`** with binary **`deepseek-runtime`**.
2. Sidecar build **without** ratatui / crossterm link.
3. Shared types in neutral modules (`agent_surface`, `auto_route`, `context_reference`).
4. Zagens **`externalBin`** → **`deepseek-runtime-*`**.

### Phase B (2026-05-26)

1. **`[lib] name = deepseek_runtime`** in `crates/runtime-server`; bin calls `deepseek_runtime::runtime_serve`.
2. Move former `crates/tui/src/*` (minus TUI tree) into `crates/runtime-server/src/`.
3. Delete **`crates/cli`** and **`crates/tui`**.
4. **`deepseek-state`:** retain in workspace — `deepseek-core` still compiles against it; **not** sidecar HTTP SSOT.

## Acceptance

### Phase A

- [x] `cargo check -p deepseek-runtime-server` green; `cargo tree -p deepseek-runtime-server -i ratatui` → **no match**
- [x] Desktop `build.rs` / `prepare-bundle.mjs` / `sidecar.rs` prefer `deepseek-runtime`
- [x] Binary contract test + CI — [`sidecar_binary_contract.rs`](../../../crates/runtime-server/tests/sidecar_binary_contract.rs)

### Phase B

- [x] `runtime_api/*` + `runtime_threads/*` under `crates/runtime-server/src/`
- [x] Workspace has **no** `crates/cli` or `crates/tui`
- [x] `export-runtime-openapi` bin in `crates/runtime-server`
- [x] [RUNTIME_ARCHITECTURE.md](../RUNTIME_ARCHITECTURE.md) updated
- [x] `RUSTFLAGS=-Dwarnings cargo check --workspace` green

## Non-goals (defer)

- Delete `deepseek-state` while `deepseek-core` still references it
- Merge sidecar into Tauri process
- P2 multi-sidecar (D11–D14)
