# M7 / M8 Handoff — Engine struct → `deepseek-core` (final two strangler PRs)

> **Status:** Open handoff (created 2026-05-25 after M6 commit `06ab12e`).
> **Audience:** Next coding session(s). Read this first before opening
> M7 or M8.
> **Parent docs:**
> - [PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md](./PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md) — authoritative plan (§3 ownership table, §5 risks, §6 PR sequence)
> - [BACKLOG_ENGINE_STRUCT_IN_CORE.md](./BACKLOG_ENGINE_STRUCT_IN_CORE.md) — progress table
> - [ARCHITECTURE_ASSESSMENT_2026-05-25.md](./ARCHITECTURE_ASSESSMENT_2026-05-25.md) §1 / §3.4 / §5 D5

## 0. Where we are (2026-05-25, after M6)

| PR | Status | Commit | Net LOC |
|----|--------|--------|---------|
| M1 — `Op`/`EngineHandle`/`ThreadContextSnapshot` to core | ✅ landed | `16e22eb` | +99 |
| M2 — `EngineConfig` split (lean/ext) | ✅ landed | `0f445a7` | +378 |
| M3 — `LspHost`/`SubAgentHost`/`ShellHost`/`SandboxHost` traits | ✅ landed | `1db7a51` | ~+320 |
| M4 — `McpHost` trait | ✅ landed | `7b979de` | ~+275 |
| M5 — `SeamHost`/`WorkshopHost`/`TopicMemoryHost` + `ScratchpadStepState` | ✅ landed | `d13f9ea` | ~+493 |
| M6 — `CapacityController` + coherence reducer atomic move | ✅ landed | `06ab12e` | ~+75 |
| **M7 — `Engine` struct + `engine_new` + `op_handlers` to core** | ⏳ **queued** | — | ≤700 cap |
| **M8 — `op_loop` to core + final cleanup** | ⏳ **queued** | — | ≤700 cap |

**Everything M3–M6 needs is in place.** All 8 subsystem boundaries
(LSP / SubAgent / Shell / Sandbox / MCP / Seam / Workshop / TopicMemory)
have `deepseek_core::engine::hosts::*` traits. Capacity controller +
coherence reducer are in core. The `Engine` struct can now be lifted
piecewise without touching subsystem implementations.

## 1. M7 scope — the big move

### What moves

Per spike §3 ownership table + §6 row M7:

1. **`Engine` struct definition** (`crates/tui/src/core/engine.rs:88-121`, 35 fields) → `deepseek_core::engine::Engine`. Field types should reference the hosts via trait objects (`Box<dyn LspHost>`, `Option<Box<dyn SeamHost>>`, `Option<Box<dyn McpHost>>`, etc.) — **not** the concrete tui types. This is the payoff of M3–M6.

2. **`Engine::new(config, api_config)`** (`crates/tui/src/core/engine/engine_new.rs`, 209 LOC) → become **`Engine::with_hosts(config_lean, hosts)`** core-side. The fat constructor that touches tui-specific things (LlmClient factory, MCP pool init, shell manager creation, sandbox backend factory, LSP manager wire-up) becomes a **tui-side builder** `pub fn spawn_engine(config: EngineConfig, api_config: &Config) -> EngineHandle` that:
   - reads tui config
   - constructs concrete subsystem instances (`LspManager`, `SubAgentManager`, `ShellManager`, `SandboxBackend`, `McpPool`, `SeamManager`, `WorkshopVariables`, `TopicMemoryRuntime`, `CapacityController`)
   - boxes them into trait objects
   - hands the bundle to `Engine::with_hosts(config.lean(), hosts)`
   - returns the `EngineHandle`

3. **`op_handlers.rs`** (`crates/tui/src/core/engine/op_handlers.rs`) → `deepseek_core::engine::op_handlers`. The handlers that don't touch tui-specific types (most `Op` variants) move directly. Any handlers that DO touch tui types (e.g. ones reading `crate::config::Config` or rendering ratatui-flavored events) need to be split into a core trait method + tui impl, OR stay tui and get called via a host trait.

### What tui keeps (`crates/tui/src/core/engine.rs` target ≤80 LOC)

After M7:

```rust
//! tui-side Engine builder + re-exports (post-M7 shim).

pub use deepseek_core::engine::Engine;
pub use deepseek_core::engine::handle::EngineHandle;

// Tui-side builder that wires concrete subsystem implementations into
// the core Engine struct and spawns the event loop on the supervised
// runtime.
pub fn spawn_engine(config: EngineConfig, api_config: &Config) -> EngineHandle {
    let (engine, handle) = build_engine(config, api_config);
    spawn_supervised("engine-event-loop", /* ... */, async move {
        engine.run().await;
    });
    handle
}

fn build_engine(config: EngineConfig, api_config: &Config) -> (Engine, EngineHandle) {
    // Build hosts: LspManager, SubAgentManager, ShellManager, Sandbox, MCP, Seam, Workshop, TopicMemory, Capacity
    // Box them, hand to Engine::with_hosts
}
```

The ~120 LOC `Engine` struct definition + 209 LOC `engine_new.rs` body
(331 LOC total) collapses into ≤80 LOC of tui builder + re-exports.

### What `EngineHostBundle` looks like (proposed)

Likely needed as a parameter pack so `Engine::with_hosts` doesn't take
20 positional arguments. Suggested signature:

```rust
// In deepseek-core::engine
pub struct EngineHostBundle {
    pub lsp: Box<dyn LspHost>,
    pub subagent: Box<dyn SubAgentHost>,
    pub shell: Box<dyn ShellHost>,
    pub sandbox: Box<dyn SandboxHost>,
    pub mcp: Option<Box<dyn McpHost>>,
    pub seam: Option<Box<dyn SeamHost>>,
    pub workshop: Box<dyn WorkshopHost>,
    pub topic_memory: Box<dyn TopicMemoryHost>,
    pub capacity_controller: CapacityController,
    // Channels (tui-built) handed in:
    pub channels: EngineChannelBundle,
    // Clients (tui-built):
    pub deepseek_client: Arc<dyn LlmClient>,
    pub deepseek_client_error: Option<String>,
    pub api_key_env_only_recovery: Option<String>,
}

impl Engine {
    pub fn with_hosts(
        config: deepseek_core::engine::EngineConfig, // lean
        hosts: EngineHostBundle,
    ) -> (Self, EngineHandle) {
        // ...
    }
}
```

`EngineChannelBundle` would wrap the 7 channels currently constructed
in `Engine::new` (`rx_op`, `tx_approval`, `rx_approval`, `rx_user_input`,
`rx_steer`, `tx_event`, `tx_subagent_completion`).

### Acceptance per spike §6 M7

- All §6 regression commands green (see §3 below).
- G2 §1–§9 manual smoke (interactive — may need user to run).
- **`crates/tui/src/core/engine.rs` ≤ 80 LOC** (re-export + spawn_engine builder only).
- Net diff ≤ 700 LOC.

### Risk surface (mostly R1, R5, R8, R11 from spike §5)

- **R1 (call-graph drift)**: M3–M6 traits were designed strictly from `&self.field.method(...)` call sites. If any host method got missed, `Engine::with_hosts` will fail to compile. Expected — fix by adding the missing method to the appropriate host trait (don't reach back through `Box<dyn> as &SeamManager`).
- **R5 (orphan rule)**: `impl TurnEnginePort for EngineHandle<P,R>` already core-side. `impl turn_loop::TurnLoopHost for Engine` is tui-side today; after M7 the `Engine` struct lives core-side, so the impl needs to either move with it (preferred — core can impl its own traits) or stay tui via a wrapper. Read `crates/tui/src/core/engine/turn_loop/host_impl/mod.rs` early.
- **R8 (LlmClient factory)**: `Engine::new` currently builds the `DeepSeekClient` from `api_config`. This factory uses tui-side `crate::client::DeepSeekClient`. Solution: factory stays tui, returns `Arc<dyn LlmClient>` (the trait IS in core); tui builder passes the boxed client into `EngineHostBundle.deepseek_client`.
- **R11 (channel ownership)**: 7 mpsc channels currently created inside `Engine::new`. They need to either be created core-side (channels are std types, fine) or be created tui-side and handed in. Recommend creating them core-side inside `Engine::with_hosts`, since `EngineHandle` (which owns the sender side of `Op`) is already core-side.

### Pre-existing 2 failures expected to resolve at M7

- `core::engine::tests::engine_mock_capacity_pre_request_observes_mock_and_emits_decision` (`tests.rs:2452`) — M6 was suspected to fix this but didn't. The bug is in **engine-flow wiring** (capacity decisions flow from `capacity_flow/observation.rs` through `Engine` state) — that wiring rewrite IS M7. **Verify resolution after M7 lands.**
- `core::engine::tests::refresh_system_prompt_under_capacity_omits_topic_memory_block` (`tests.rs:991`) — likely needs the M7 Engine wiring to expose the topic_memory injection path correctly via the `TopicMemoryHost` trait. Also verify post-M7.

If they persist after M7, file as `M8.1 follow-up` (probably actual logic bug in the path, not a structural issue).

## 2. M8 scope — closeout

Per spike §6 row M8:

1. **`op_loop.rs`** (`crates/tui/src/core/engine/op_loop.rs`) → `deepseek_core::engine::op_loop`. This is the main event loop body. After M7 the Engine struct is core-side, so `op_loop` finally has somewhere to land.
2. **Remaining `Engine::handle_*_op` helpers**:
   - `compaction_ops.rs`
   - `edit_turn_ops.rs`
   - `session_ops.rs`
   - `subagent_spawn.rs` (if not absorbed by M7)
3. **Roadmap updates**: §17.1 / §17.5 of `RUNTIME_EVOLUTION_ROADMAP.md`.
4. **Close [BACKLOG_ENGINE_STRUCT_IN_CORE.md](./BACKLOG_ENGINE_STRUCT_IN_CORE.md)** — status → `Closed`.
5. **Delete this handoff doc** in the M8 commit (its job is done).
6. **Reassessment time**: per `ARCHITECTURE_ASSESSMENT_2026-05-25.md` §8, regenerate the file as `ARCHITECTURE_ASSESSMENT_<post-M8-date>.md` v2 with §1 checklist re-evaluated. Likely §1 #4 (Engine struct in core) and #5 (sidecar not linking ratatui) both flip to `[x]`.

### Acceptance per spike §6 M8

- All §6 regression + sidecar contract + multi-window manual smoke green.
- `cargo run -p deepseek-tui` starts cleanly.
- `cargo run -p deepseek-tui --bin sidecar` starts cleanly.
- 2 pre-existing test failures resolved (if M7 didn't already).
- BACKLOG promoted to Closed.

### Optional follow-up (post-M8, not blocking)

- Extract `crates/runtime-server` (sidecar binary without ratatui/CLI dep) per ASSESSMENT D6. This is the next backlog item once Engine is fully in core — `crates/tui-core` is already deleted (2026-05-25), so the sidecar binary inheriting just `deepseek-core + http + lsp` becomes feasible.

## 3. §6 regression block (run after every M-series step)

Spike §6 mandates these commands green for every M-step. Use this
verbatim as your smoke test after M7 + M8 land:

```powershell
# Core build + scoped tests
cargo build -p deepseek-core
cargo test -p deepseek-core --lib capacity
cargo test -p deepseek-core --lib coherence
cargo test -p deepseek-core --lib engine::turn_loop::capacity_policy
cargo test -p deepseek-core --lib engine::scratchpad_state
cargo test -p deepseek-core --lib engine::hosts

# tui build + targeted regression
cargo build -p deepseek-tui
cargo test -p deepseek-tui --lib mcp
cargo test -p deepseek-tui --lib tools::subagent
cargo test -p deepseek-tui --lib seam_manager
cargo test -p deepseek-tui --lib compaction
cargo test -p deepseek-tui --lib scratchpad
cargo test -p deepseek-tui --lib history_isomorphism
cargo test -p deepseek-tui --lib capacity_escalation
cargo test -p deepseek-tui --lib core::capacity_memory
cargo test -p deepseek-tui --lib runtime_api::tests::sidecar_contract_full_lifecycle
cargo test -p deepseek-tui --test protocol_recovery

# Web UI sanity
cd crates/desktop/web-ui
npm run test:f3
npm run build
```

**Pre-existing failures (carried since M3)** — both expected to clear
at M7 OR be diagnosed as actual logic bugs:
- `core::engine::tests::refresh_system_prompt_under_capacity_omits_topic_memory_block` (tests.rs:991)
- `core::engine::tests::engine_mock_capacity_pre_request_observes_mock_and_emits_decision` (tests.rs:2452)

If either still fails after M7, that's a logic bug to fix in M8 cleanup.

## 4. Key files to read first (recommended reading order)

For a new session opening M7, read in this order:

1. **`docs/tech/adr/PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md`** — §0 TL;DR + §3 ownership table (35 fields) + §5 risk table + §6 PR sequence. This is the authoritative spec.
2. **This file** (you're reading it).
3. **`crates/tui/src/core/engine.rs`** — Engine struct definition (192 LOC). All 35 fields are here. Note which already use trait/host types (post-M3/M4/M5/M6) vs which are still concrete tui types.
4. **`crates/tui/src/core/engine/engine_new.rs`** — `Engine::new` constructor (209 LOC). This is what gets split into core `Engine::with_hosts` + tui `spawn_engine` builder.
5. **`crates/tui/src/core/engine/op_handlers.rs`** — handler dispatch (size TBD by new session).
6. **`crates/core/src/engine/mod.rs`** — what's already core-side (Op enum, EngineHandle, hosts, config, capacity, coherence, scratchpad_state).
7. **`crates/core/src/engine/hosts/mod.rs`** — 8 host traits ready to be plugged in.
8. **`crates/tui/src/core/engine/turn_loop/host_impl/mod.rs`** — existing `impl TurnLoopHost for Engine` (will need to move/adapt).

For M8, also read:
9. **`crates/tui/src/core/engine/op_loop.rs`** — main event loop.

## 5. Build environment notes

- Windows / PowerShell: heredoc `<<'EOF'` does NOT work for git commit messages. Write commit message to `.tmp_commit_msg.txt`, `git commit -F .tmp_commit_msg.txt`, then `Delete` the file.
- `cargo test -p deepseek-tui` cold build is ~90s; subsequent incremental ~2s. Schedule patience accordingly.
- LF/CRLF warnings on every tui write are noise — ignore.

## 6. M-series invariants (must hold for M7 + M8)

Per spike §1.1 + §5:

- **No breaking `/v1/*` HTTP changes** — sidecar contract test gates this.
- **No tool registry surgery** — tools stay tui; M-series only moves Engine + supporting state.
- **≤700 LOC net per PR** — if M7 grows too big, split into M7a (struct + new) and M7b (op_handlers).
- **R1 call-graph driven** — only add trait methods that Engine actually calls today.
- **No double implementation** — when moving a type to core, delete the tui original in the same PR. Use re-export shim if call sites need the old path.

## 7. Handoff cleanup checklist (delete this doc when done)

- [ ] M7 commit landed with ≤80 LOC `tui/src/core/engine.rs` shim
- [ ] M8 commit landed
- [ ] `BACKLOG_ENGINE_STRUCT_IN_CORE.md` status → Closed
- [ ] Optional: `ARCHITECTURE_ASSESSMENT_<date>.md` v2 regenerated
- [ ] **`docs/tech/adr/HANDOFF_M7_M8.md` deleted** in M8 commit
