# D6 — `deepseek-runtime` sidecar (runtime-server crate)

**Status:** Landed (phase A — 2026-05-26)  
**Related:** [ARCHITECTURE_ASSESSMENT_2026-05-25.md](./ARCHITECTURE_ASSESSMENT_2026-05-25.md) §1 #5 · §5.1 阶段 A

## Context

Zagens desktop embeds a local HTTP sidecar. Before D6, the bundled binary was `deepseek-tui serve --http`, linking **ratatui**, crossterm, and the full CLI surface — contradicting the Desktop-only / sidecar-first architecture.

M-series (D5) moved `Engine` + op loop into `deepseek-core`, removing the structural blocker for a headless runtime binary.

## Decision

1. Add workspace crate **`deepseek-runtime-server`** with binary **`deepseek-runtime`**.
2. Entry: `deepseek_tui::runtime_serve` — flat CLI (`--host`, `--port`, …), no `serve` subcommand.
3. **`deepseek-tui` library** gains feature **`tui-ui`** (default on) gating ratatui / crossterm / `mod tui` / `commands` / `palette` / `config_ui` / `deepseek_theme`.
4. Sidecar build uses `deepseek-tui` with **`default-features = false`** — **no ratatui link**.
5. Shared types extracted to **`agent_surface`**, **`auto_route`**, **`context_reference`** so HTTP runtime paths compile without the TUI module tree.
6. Zagens **`externalBin`** switches from `deepseek-tui-*` to **`deepseek-runtime-*`**; supervisor keeps legacy `deepseek-tui serve --http` argv when dev fallback detects the old binary name.

## Acceptance (phase A)

- [x] `cargo check -p deepseek-runtime-server` green; `cargo tree -p deepseek-runtime-server -i ratatui` → **no match**
- [x] `cargo check -p deepseek-tui --bin deepseek-tui` green (full TUI + CLI unchanged)
- [x] Desktop `build.rs` / `prepare-bundle.mjs` / `sidecar.rs` prefer `deepseek-runtime`
- [ ] `runtime_api/tests.rs` sidecar contract run against **`deepseek-runtime`** binary (follow-up CI wiring)
- [ ] Physical move of `runtime_api/*` + `runtime_threads/*` into `runtime-server` **library** (phase B — optional; not blocking §1 #5)

## Non-goals (defer)

- Delete `deepseek-tui serve --http` (keep for CLI/dev fallback)
- Split `runtime_api` out of `crates/tui` src tree (D6 phase B / D1 follow-up)
