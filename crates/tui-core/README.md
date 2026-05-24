# deepseek-tui-core (legacy)

**Status:** Retained for **snapshot tests only** (`cargo test -p deepseek-tui-core`). Not linked by `deepseek-tui`, DS Pick, or the production CLI.

**B3 decision (2026-05-24):** Do **not** extend this crate for new UI state. New shared TUI types belong in `deepseek-tui` library modules or `deepseek-core` when runtime-facing.

See [RUNTIME_EVOLUTION_ROADMAP.md](../docs/tech/RUNTIME_EVOLUTION_ROADMAP.md) §9.3 B3.2.
