# PR-M0 Spike — Engine struct → `deepseek-core` (M-series strangler plan)

> **Status:** Draft (2026-05-25) — spike output, **no code changes**.  
> **Owner:** runtime / Engine refactor working group.  
> **Roadmap:** [RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md) §11.0 ADR backlog + §17.3.  
> **Parent ADR:** [BACKLOG_ENGINE_STRUCT_IN_CORE.md](./BACKLOG_ENGINE_STRUCT_IN_CORE.md) — promoted from "proposed" to "in spike" by this doc.

## 0. TL;DR

P2 PR6 (G3) already moved `turn_loop` phases into `deepseek-core`. **What is
left in `crates/tui` for the `Engine` *struct* migration is mostly
subsystems, not loop logic**:

| Piece | Status |
|-------|--------|
| `turn_loop/*` (streaming, tools, capacity policy) | **✅ in core** |
| `Session`, `Compaction`, `Capacity*Config`, `ApprovalMode`, `TurnLoopMode`, `Event`, `LlmClient`, `Scratchpad*` types | **✅ in core (re-exported by tui)** |
| `TurnLoopHost` trait + `TurnLoopToolRegistry` / `TurnLoopMcpPool` markers | **✅ in core** |
| `Engine` struct (35 fields, 7 channels, 6 concrete subsystems) | **🟡 still tui** — main M-series target |
| `Op` enum (15 variants) + `EngineHandle` | **🟡 tui (Op refs `AppMode`, `ApprovalMode`)** |
| `op_loop` / `op_handlers` / `engine_new` | **🟡 tui** (200–222 LOC each) |
| `MCP`, `LSP`, `Sandbox`, `Subagent manager`, `Shell manager`, `Seam`, `Cycle`, `Topic memory`, `Workshop` subsystems | **🔴 tui-only** — must become traits (or skip move) |

**ADR draft says:** "Defer whole-struct migration until tool/MCP boundaries
are trait-stable. Prefer incremental port of session op queue types, not a
monolithic move." This spike confirms that ADR position and produces a
strangler plan that obeys it.

**Definition of done (target acceptance):** `crates/tui/src/core/engine.rs`
holds **only** re-exports + a tui-side host builder. The struct, op loop,
and turn_loop_host trait all live in `deepseek-core::engine`. `/v1/*`
contract tests stay green throughout.

## 1. Scope & invariants

### 1.1 Hard invariants (any PR that breaks one is **rejected**)

1. **No `/v1` breaking changes.** Sidecar contract test
   `sidecar_contract_full_lifecycle` + `event_schema_version: 2` MUST stay
   green. Behavior-only changes — wire format stays identical.
2. **Sidecar binary** remains `deepseek-tui` (Roadmap D11).
3. **Tools stay in tui.** `crates/tui/src/tools/*` (~28k LOC across
   ~55 files including `file.rs` 2269, `shell.rs` 2572, `apply_patch.rs`
   1489, `web_run.rs` 1826, …) does **not** move. Core depends only on
   trait boundaries.
4. **`/v1` HTTP layer** (`runtime_api/*`, `runtime_threads/*`) is only
   touched to swap import paths — no behavior change.
5. **Each PR ≤ ~700 lines net.** Hard cap to keep review tractable; if a
   step needs more, **split**.
6. **Each PR ships green** — `cargo build`, the §6 regression command
   block from `IMPLEMENTATION_SUMMARY_2026-05-24.md`, and `cargo test -p
   deepseek-tui --lib sidecar_contract_full_lifecycle` MUST all pass on
   the PR before merge.

### 1.2 Out of scope for the M-series

- Replacing `app-server` sidecar (D4 frozen).
- Unifying `StateStore` ↔ `runtime_threads` JSONL (separate backlog ADR
  [BACKLOG_STATESTORE_JSONL.md](./BACKLOG_STATESTORE_JSONL.md)).
- Moving any `crates/tui/src/tools/*` implementation into core.
- Landlock / Windows sandbox enforcement (separate backlog ADR
  [BACKLOG_LANDLOCK_ENFORCE.md](./BACKLOG_LANDLOCK_ENFORCE.md)).
- Replacing `seam_manager` or `cycle_manager` — these stay tui-owned and
  bridge through new traits.

### 1.3 Code-size sanity (anchor for "no big-bang")

| Path | Lines | Notes |
|------|-------|-------|
| `crates/tui/src/core/engine.rs` | 209 | The "thin shell" — but every field still concrete tui types |
| `crates/tui/src/core/engine/` (recursive, excl. `tests.rs`) | ~5.0k | Submodules incl. `host_impl/mod.rs` 533, `scratchpad_flow.rs` 484, `turn_loop/tool_plans_exec.rs` 454 |
| `crates/tui/src/core/engine/tests.rs` | 2495 | Stays with engine wherever it ends up |
| `crates/tui/src/mcp.rs` | 2218 | Trait surface only — no move |
| `crates/tui/src/lsp/` | ~1349 | Trait surface only — no move |
| `crates/tui/src/sandbox/` | ~2070 | `SandboxBackend` is **already a dyn trait** ✅ |
| `crates/tui/src/seam_manager.rs` | 712 | Trait surface only — no move |
| `crates/tui/src/cycle_manager.rs` | 993 | Trait surface only — no move |
| `crates/core/src/engine/` (excl. tests) | ~4.0k | Existing turn_loop / approval / dispatch / context |
| `crates/core/src/lib.rs` | 1956 | Already in "L2 terminal state"; new modules to be added |

## 2. Dependency graph

### 2.1 Current (after P2 PR6 / G3, 2026-05-23)

```
┌─────────────────────────────────────────────────────────────────────┐
│ deepseek-tui                                                        │
│                                                                     │
│  ┌────────────────────────┐         ┌─────────────────────────────┐ │
│  │ Engine struct (35 fld) │ owns──► │ subagent_manager (tools)    │ │
│  │ engine.rs 209          │         │ shell_manager (tools)       │ │
│  │ engine/engine_new 222  │         │ mcp_pool: McpPool (2218)    │ │
│  │ engine/op_loop 88      │         │ lsp_manager: LspManager     │ │
│  │ engine/op_handlers 67  │         │ sandbox_backend: dyn trait  │ │
│  │ engine/handle 137      │         │ seam_manager (712)          │ │
│  │ host_impl/ 533+        │         │ workshop_vars (~604)        │ │
│  │ scratchpad_flow 484    │         │ topic_memory_runtime        │ │
│  │ tool_plans_exec 454    │         │ capacity_controller         │ │
│  └────┬───────────────────┘         │ pending_lsp_blocks: Vec     │ │
│       │                             └─────────────────────────────┘ │
│       │ impl TurnLoopHost                                           │
│       ▼                                                             │
│  ┌─ ports re-export ──────────────────────────────────────────────┐ │
│  │ Op enum (tui/core/ops.rs) — refs AppMode (tui), ApprovalMode   │ │
│  │ EngineHandle (tui/core/engine/handle.rs)                       │ │
│  └────┬───────────────────────────────────────────────────────────┘ │
│       │                                                             │
└───────┼─────────────────────────────────────────────────────────────┘
        │ deepseek-core depends inward
        ▼
┌─────────────────────────────────────────────────────────────────────┐
│ deepseek-core                                                       │
│  ✅ engine/turn_loop/{run,host,streaming_phase,tool_phase,...}      │
│  ✅ engine/{approval, dispatch, context, loop_guard, ...}           │
│  ✅ session, compaction, capacity, scratchpad, events               │
│  ✅ TurnLoopHost / TurnEnginePort / SubAgentSpawnPort traits        │
│  ❌ Engine struct                                                   │
│  ❌ Op enum, EngineHandle                                           │
│  ❌ op_loop, op_handlers                                            │
└─────────────────────────────────────────────────────────────────────┘
```

### 2.2 Target (post M-series)

```
┌─────────────────────────────────────────────────────────────────────┐
│ deepseek-tui (thin shell + concrete subsystems)                     │
│                                                                     │
│  crates/tui/src/core/engine.rs   ← re-exports + spawn_engine wiring │
│  crates/tui/src/core/host/       ← concrete trait impls             │
│    ├─ mcp.rs       impl McpHost          (wraps tui::mcp::McpPool)  │
│    ├─ lsp.rs       impl LspHost          (wraps lsp::LspManager)    │
│    ├─ sandbox.rs   impl SandboxHost      (wraps dyn SandboxBackend) │
│    ├─ subagent.rs  impl SubAgentHost     (wraps SharedSubAgentMgr)  │
│    ├─ shell.rs     impl ShellHost        (wraps SharedShellManager) │
│    ├─ seam.rs      impl SeamHost         (wraps SeamManager)        │
│    ├─ cycle.rs     impl CycleHost        (wraps CycleManager state) │
│    ├─ workshop.rs  impl WorkshopHost     (wraps WorkshopVariables)  │
│    └─ topic_mem.rs impl TopicMemoryHost  (wraps TopicMemoryRuntime) │
│                                                                     │
└──────────────────────────────────┬──────────────────────────────────┘
                                   │ trait surface only
                                   ▼
┌─────────────────────────────────────────────────────────────────────┐
│ deepseek-core                                                       │
│                                                                     │
│  engine/{                                                           │
│    runtime.rs            ← struct Engine<HostBundle>                │
│    runtime_new.rs        ← Engine::new(config, hosts)               │
│    op_loop.rs            ← Op dispatch loop                         │
│    op_handlers.rs        ← non-turn ops                             │
│    engine_handle.rs      ← EngineHandle                             │
│    op.rs                 ← Op enum (no AppMode dep)                 │
│    hosts/                ← new trait module                         │
│      mod.rs              ← `HostBundle` aggregator trait            │
│      mcp.rs              ← trait McpHost                            │
│      lsp.rs              ← trait LspHost + DiagnosticBlock          │
│      sandbox.rs          ← (uses existing dyn SandboxBackend)       │
│      subagent.rs         ← trait SubAgentHost                       │
│      shell.rs            ← trait ShellHost                          │
│      seam.rs             ← trait SeamHost                           │
│      cycle.rs            ← trait CycleHost                          │
│      workshop.rs         ← trait WorkshopHost                       │
│      topic_memory.rs     ← trait TopicMemoryHost                    │
│  }                                                                  │
└─────────────────────────────────────────────────────────────────────┘
```

## 3. Engine struct field-ownership table

35 fields total (declared `crates/tui/src/core/engine.rs` L87–L121). Each
row maps a current tui-typed field to its M-series destination.

| # | Field | Current type (tui) | Target home | Bridging strategy | M-PR |
|---|-------|-------------------|-------------|-------------------|------|
| 1 | `config` | `EngineConfig` (tui types/cfg leak: `NetworkPolicyDecider`, `lsp_config`, `workshop`, `task_type`, `topic_memory::TopicMemorySettings`) | core (lean `EngineConfig`) + tui side-car for ext fields | Split: keep "fat" `EngineConfigExt` tui-side; pass slim `core::engine::EngineConfig` plus trait objects | M2 |
| 2 | `deepseek_client` | `Option<Arc<dyn LlmClient>>` | **already core trait** | None | — |
| 3 | `deepseek_client_error` | `Option<String>` | core (plain) | None | M2 |
| 4 | `api_key_env_only_recovery` | `Option<String>` | core (plain) | None | M2 |
| 5 | `session` | `Session` (core re-exp) | **already core** | None | — |
| 6 | `subagent_manager` | `SharedSubAgentManager` (tui::tools::subagent) | tui-only | new `trait SubAgentHost` (spawn, list, completion stream) | M3 |
| 7 | `shell_manager` | `SharedShellManager` (tui::tools::shell) | tui-only | new `trait ShellHost` (acquire, register progress) | M3 |
| 8 | `mcp_pool` | `Option<Arc<AsyncMutex<McpPool>>>` (2218 LOC) | tui-only | new `trait McpHost` (lazy build, is_mcp_tool, dispatch); replace existing `TurnLoopMcpPool` marker with named trait | M4 |
| 9 | `rx_op` | `mpsc::Receiver<Op>` | core | `Op` enum + receiver move; needs §3.1 fix for AppMode | M2 + M5 |
| 10 | `tx_approval` | `mpsc::Sender<ApprovalDecision>` (core generic over `SandboxPolicy`) | core (already generic) | Specialize tui-side: `ApprovalDecision<SandboxPolicy>` | M2 |
| 11 | `rx_approval` | same as #10 | core | same | M2 |
| 12 | `rx_user_input` | `mpsc::Receiver<UserInputDecision>` (core generic) | core (already generic) | Specialize tui-side: `UserInputDecision<UserInputResponse>` (UserInputResponse stays tui) | M2 |
| 13 | `rx_steer` | `mpsc::Receiver<String>` | core | None | M2 |
| 14 | `tx_event` | `mpsc::Sender<Event>` (core re-exp) | **already core** | None | — |
| 15 | `tx_subagent_completion` | `mpsc::UnboundedSender<SubAgentCompletion>` (tui::tools::subagent) | tui (channel ends tui) | Owned by `SubAgentHost` impl, not Engine | M3 |
| 16 | `rx_subagent_completion` | `mpsc::UnboundedReceiver<...>` | tui | same | M3 |
| 17 | `cancel_token` | `CancellationToken` | **already core** | None | — |
| 18 | `shared_cancel_token` | `Arc<StdMutex<CancellationToken>>` | core | None | M2 |
| 19 | `tool_exec_lock` | `Arc<RwLock<()>>` | core | None | M2 |
| 20 | `capacity_controller` | `CapacityController` (tui::core::capacity, 686 LOC; config in core) | core | Move `CapacityController` struct itself into core (already partly there) | M6 |
| 21 | `seam_manager` | `Option<SeamManager>` (712 LOC) | tui-only | new `trait SeamHost` (process_seam, refresh_layer) | M5 |
| 22 | `coherence_state` | `CoherenceState` (tui::core::coherence, 91 LOC) | core (small enum + struct) | Move into core directly | M2 |
| 23 | `turn_counter` | `u64` | core | None | M2 |
| 24 | `lsp_manager` | `Arc<LspManager>` (1349 LOC) | tui-only | new `trait LspHost` (run_diagnostics, post_edit_hook) | M3 |
| 25 | `workshop_vars` | `Option<Arc<Mutex<WorkshopVariables>>>` (large_output_router) | tui-only | new `trait WorkshopHost` (alloc_ref, vars_snapshot) | M5 |
| 26 | `sandbox_backend` | `Option<Arc<dyn SandboxBackend>>` | **already a dyn trait** | Move trait def into core; impl stays tui | M3 |
| 27 | `pending_lsp_blocks` | `Vec<DiagnosticBlock>` (tui::lsp) | core (plain Vec) | `DiagnosticBlock` becomes core type | M3 |
| 28 | `scratchpad_step` | `scratchpad_flow::ScratchpadStepState` (tui-only) | core (plain state struct) | Move state to core; tui keeps high-level flow helpers | M5 |
| 29 | `scratchpad_run_id` | `Option<String>` | core | None | M5 |
| 30 | `scratchpad_summary_injected_this_turn` | `bool` | core | None | M5 |
| 31 | `topic_memory_runtime` | `TopicMemoryRuntime` (tui wraps deepseek-topic-memory crate) | tui-only | new `trait TopicMemoryHost`; **OR** add `deepseek-topic-memory` dep to core (cheaper) | M5 |

**Channels summary (#9–#16 + #18):** Engine owns 7 mpsc receivers/senders.
`Op` and `Event` already need to move first (M2). Approval / user-input
channels piggy-back on core-side generics that already exist (see
`crates/core/src/engine/approval.rs::ApprovalDecision<P>`).

## 4. `Op` enum migration sub-design

`Op` (15 variants, `crates/tui/src/core/ops.rs`) is the biggest single
type-flow blocker. Two AppMode-flavored variants need treatment:

- `Op::SendMessage { mode: AppMode, approval_mode: ApprovalMode, ... }`
  → use core `TurnLoopMode` instead of `AppMode` (already mirrors it,
  `crates/core/src/turn.rs:51`); `ApprovalMode` is already core
  (`crates/core/src/approval.rs:5`, re-exported by tui).
- `Op::ChangeMode { mode: AppMode }` → same `TurnLoopMode` swap.
- `Op::SyncSession { messages, system_prompt, model, workspace }` →
  fields already core types (`Message`, `SystemPrompt` via core
  re-export).
- `Op::QueryContext { reply: oneshot::Sender<ThreadContextSnapshot> }` →
  `ThreadContextSnapshot` is in `crates/tui/src/context_snapshot.rs`.
  Need to move it to core (small, ~tui-only type).

Conversion helper `app_mode_to_turn_loop` /  `turn_loop_to_app_mode` is
already in `host_impl/mod.rs:518/527` — survives the migration on the
tui boundary, not in core.

## 5. Risk register

Numbered so PRs can reference them.

| # | Risk | Probability | Impact | Mitigation |
|---|------|-------------|--------|------------|
| R1 | Trait surface for `McpHost` / `LspHost` is too narrow → host_impl panics on a missing method that turn_loop newly calls | Med | High | Build traits **from `TurnLoopHost` call sites first** (call-graph driven), not from subsystem method dumps. Add `#[deny(missing_docs)]` to spike-only-PRs to force trait designer to think. |
| R2 | `EngineConfig` is 27 fields wide with tui-only types (`NetworkPolicyDecider`, `WorkshopConfig`, `TopicMemorySettings`) — splitting it cleanly is fiddly | High | Med | Two-struct approach: `core::engine::EngineConfig` (lean) + `tui::core::EngineConfigExt` (rest). Pass both into core `Engine::new(slim, ext_via_host)`. |
| R3 | `op_loop` references `self.handle_send_message(...)` which spans 14 modules — moving alone needs trait abstractions | High | High | Land `op_loop` **last** (M-final). Move per-op handlers across in batches; the loop body shrinks naturally. |
| R4 | Multi-window sidecar regression — `runtime_threads/active.rs` + `manager.rs` directly hold `EngineHandle` and own approval channels | Med | High | Run `sidecar_parallel_pending_approvals_resolve_then_continue` + multi-window manual smoke (G2 §4) on **every** M-PR that touches handle / channel types. |
| R5 | Cycle/seam managers maintain shared in-process state (e.g. last hash, replay buffer) — naive trait extraction may break replay determinism | Med | Med | Keep state in tui side of trait impl (`Arc<Mutex<…>>` ownership unchanged); core only invokes via trait. |
| R6 | `pending_lsp_blocks: Vec<DiagnosticBlock>` is mutated by both turn_loop (via `flush_pending_lsp_diagnostics`) and host op handlers — splitting ownership is non-trivial | Low | Med | Move `DiagnosticBlock` to core as a plain serde type. Vec ownership stays on Engine (in core). LSP-specific extraction stays in `LspHost`. |
| R7 | `Engine` borrowing currently relies on field-disjoint `&mut self` patterns (e.g. `session_mut` + `tx_event` + `rx_steer_mut` in TurnLoopHost). A `HostBundle` trait must preserve disjoint-borrow ergonomics | High | Med | Mirror existing `TurnLoopHost`-style methods (one accessor per field); avoid putting hosts behind a single `&dyn HostBundle` getter. Each host stored as its own field in core Engine. |
| R8 | `tests.rs` 2495 LOC depends on tui-side `Engine::new` shortcuts | High | Low | Keep `tests.rs` tui-side; add a thin `core::engine::tests::*` for new code only. tui tests stay green via re-exports. |
| R9 | Workshop / topic-memory traits would force core to depend on `deepseek-topic-memory` | Low | Low | Either (a) keep adapters tui-side via `WorkshopHost` / `TopicMemoryHost`, **or** (b) add `deepseek-topic-memory` to core deps (cheap — already a workspace crate). Prefer (a) for layering purity. |
| R10 | Capacity controller is 686 LOC and partly already in core; double-implementation risk | Med | Med | M6 = single atomic move (with `cargo test -p deepseek-core --lib capacity_policy`). Delete tui copy in same PR. |
| R11 | Tools `RuntimeToolServices` reaches deep into tui (`SharedTodoList`, `SharedPlanState`, `large_output_router::WorkshopVariables`, scratchpad run-id mutex) | Med | Med | Stays in tui; passed through `EngineConfigExt` as opaque `Arc<dyn …>` plus current `RuntimeToolServices`. core treats it as a black-box service handle. |
| R12 | Hidden circular dependency surface — `crates/tui/src/scratchpad/config.rs:3` re-exports core types; if M5 moves `scratchpad_flow` into core, tui still wants the auditor/coverage UI helpers in `scratchpad/{mod,auditor,coverage,…}` | Med | Low | Move only `ScratchpadStepState` + flow primitives, leaving UI/auditor in tui. |

## 6. PR plan (M-series, strangler)

**Sequencing principle:** Move *data types and channels* before
*subsystems*; subsystems before *the loop*; the loop last. Each PR ≤
~700 lines net. Each PR ships green on the §6 regression block from
`IMPLEMENTATION_SUMMARY_2026-05-24.md`.

| PR | Title | Scope | Acceptance | Size cap |
|----|-------|-------|------------|----------|
| **M0** | **This spike** | ADR only (this doc) + update `BACKLOG_ENGINE_STRUCT_IN_CORE.md` to "in spike". **Zero code change.** | Maintainer approves design before M1. | 0 LOC |
| **M1** | `Op` + `ThreadContextSnapshot` + `coherence` to core (no behavior change) | Move `Op` enum, `EngineHandle` (channels only), `ThreadContextSnapshot`, `CoherenceState` to `deepseek-core::engine`. tui keeps re-export shim. `AppMode` ↔ `TurnLoopMode` swap at the `Op::SendMessage` / `ChangeMode` boundary. | All §6 regression commands green. `cargo build -p deepseek-core` builds. `runtime_threads/manager.rs` only changes import paths. | ≤500 LOC net |
| **M2** | `EngineConfig` split + `tui::core::EngineConfigExt` | Move lean `EngineConfig` (model, workspace, allow_shell, trust_mode, paths, max_steps, compaction, capacity, scratchpad, locale_tag) to core. Tui-only fields (NetworkPolicyDecider, lsp_config, workshop, topic_memory, runtime_services, …) → `EngineConfigExt`. tui builder fuses them. | `engine::tests::engine_llm_client_override_runs_mock_turn` + 36 error_taxonomy golden + sidecar_contract_full_lifecycle green. | ≤700 LOC net |
| **M3** | Subsystem traits: `LspHost`, `SubAgentHost`, `ShellHost`, `SandboxBackend` re-home | Define traits in `crates/core/src/engine/hosts/`. Implement on tui types. Move `DiagnosticBlock` to core. Move `dyn SandboxBackend` trait def to core (impl stays in tui). turn_loop_host **does not change** semantics. | Manual smoke G2 §1–§3 + multi-window §4. `cargo test -p deepseek-tui --lib tools::subagent` green. | ≤700 LOC net |
| **M4** | `McpHost` trait + core call sites use trait, tui::mcp untouched | New `trait McpHost` covering: is_mcp_tool, lazy ensure, dispatch, parallel/read-only metadata. `TurnLoopMcpPool` marker replaced. **Zero changes to `crates/tui/src/mcp.rs` body**, only an `impl McpHost`. | Sidecar contract test + MCP integration test green. | ≤500 LOC net |
| **M5** | Seam / Cycle / Workshop / Topic-memory hosts + scratchpad state to core | New traits + tui-side impls. Move only `ScratchpadStepState` (~30 LOC) to core; flow helpers (`scratchpad_flow.rs` 484) stay in tui as the `SeamHost`/etc. impl helpers. | Compaction + scratchpad audit tests green. Sidecar contract test green. | ≤700 LOC net |
| **M6** | `CapacityController` struct → core (consolidate) | `tui::core::capacity::CapacityController` (686) → `deepseek-core::engine::capacity::CapacityController`. Re-export shim in tui. Delete tui original. | `cargo test -p deepseek-core --lib capacity_policy` + `cargo test -p deepseek-tui --lib capacity_escalation` green. | ≤700 LOC net |
| **M7** | `Engine` struct + `engine_new` + `op_handlers` into core | Move struct definition (with all hosts plugged through `M3`–`M6` traits), `Engine::new` (now `Engine::with_hosts(...)`), `op_handlers.rs`. tui side becomes a builder: `pub fn spawn_engine(...) → EngineHandle` that wires hosts and forwards to core. | All §6 regression + G2 §1–§9 manual smoke. `crates/tui/src/core/engine.rs` ≤ **80 LOC** (re-export + spawn_engine builder only). | ≤700 LOC net |
| **M8** | `op_loop` into core + final cleanup | Move `op_loop.rs`, remaining `Engine::handle_*_op` helpers (`compaction_ops.rs`, `edit_turn_ops.rs`, `session_ops.rs`, `subagent_spawn.rs` if applicable). Update §17.1 / §17.5 of roadmap. | All §6 regression + sidecar contract + multi-window manual smoke. Update [BACKLOG_ENGINE_STRUCT_IN_CORE.md](./BACKLOG_ENGINE_STRUCT_IN_CORE.md) to **Closed**. | ≤700 LOC net |

**Total expected:** ~7 commits over ~4–7 weeks (depending on contention
with B / GAP work). Any week the sidecar contract test goes red blocks
the next M-PR.

## 7. M1 diff scope — **file list only**

Files that M1 will touch (no code in this spike). All edits are
type-level / import-path moves; **zero behavior change** required by M1.

### 7.1 Files moved into `deepseek-core`

| New core path | Source tui path | Purpose |
|---------------|-----------------|---------|
| `crates/core/src/engine/op.rs` | `crates/tui/src/core/ops.rs` (113) | `Op` enum (15 variants), with `TurnLoopMode` swap. |
| `crates/core/src/engine/handle.rs` | `crates/tui/src/core/engine/handle.rs` (137) | `EngineHandle` + channel ends. `SandboxPolicy` becomes a generic; tui-side type alias `EngineHandle = core::EngineHandle<SandboxPolicy>` keeps callers ergonomic. |
| `crates/core/src/engine/context_snapshot.rs` | `crates/tui/src/context_snapshot.rs` (verify size on M1) | `ThreadContextSnapshot` is small + serde-only — clean move. |
| `crates/core/src/coherence.rs` | `crates/tui/src/core/coherence.rs` (~91) | Already small + tui-only deps are zero — direct move. |

### 7.2 Files edited (import-path only)

| File | Edit type |
|------|-----------|
| `crates/tui/src/core/ops.rs` | Replace body with `pub use deepseek_core::engine::op::Op;` shim. |
| `crates/tui/src/core/engine/handle.rs` | Same — re-export `EngineHandle` shim. |
| `crates/tui/src/context_snapshot.rs` | Re-export shim. |
| `crates/tui/src/core/coherence.rs` | Re-export shim. |
| `crates/tui/src/core/engine.rs` | Update `use super::ops::Op;` → already comes via re-export; verify channel `mpsc::Receiver<Op>` resolves. |
| `crates/tui/src/runtime_threads/manager.rs` | No code; just confirm `EngineHandle` import path still resolves. |
| `crates/tui/src/runtime_threads/active.rs` | Same. |
| `crates/tui/src/runtime_threads/monitor.rs` | Same. |
| `crates/tui/src/runtime_threads/engine_load.rs` | Same. |
| `crates/tui/src/core/engine/op_loop.rs` | Confirm `match op { Op::… }` arms still compile (uses re-exported enum). |
| `crates/tui/src/core/engine/op_handlers.rs` | Same. |
| `crates/tui/src/cli/commands/legacy.rs` | Same. |
| `crates/tui/src/tui/ui.rs` | Same. |
| `crates/tui/src/retry_status.rs` | Same. |
| `crates/tui/src/task_manager.rs` | Same. |
| `crates/core/src/engine/mod.rs` | Add `pub mod op; pub mod handle; pub mod context_snapshot;` + re-exports for tui. |
| `crates/core/src/lib.rs` | If `coherence.rs` lands at core root, add `pub mod coherence;`. |

### 7.3 Files **NOT** touched in M1 (sanity assertion)

- All of `crates/tui/src/tools/*`
- `crates/tui/src/mcp.rs`
- `crates/tui/src/lsp/*`
- `crates/tui/src/sandbox/*`
- `crates/tui/src/seam_manager.rs`
- `crates/tui/src/cycle_manager.rs`
- `crates/desktop/*` (Zagens) — no contract change at this layer
- `crates/tui/src/runtime_api/*` — HTTP wire format frozen
- All `tests.rs` / test fixtures — they consume re-exported types

If M1's diff touches anything outside §7.1/§7.2, **the PR is too big**
and must be split or scoped down.

### 7.4 M1 acceptance checklist

- [ ] `cargo build -p deepseek-core` clean.
- [ ] `cargo build -p deepseek-tui` clean (only re-export shims new).
- [ ] §6 regression block from `IMPLEMENTATION_SUMMARY_2026-05-24.md`
      green:
  - `cargo test -p deepseek-core --lib capacity_policy`
  - `cargo test -p deepseek-tui --lib history_isomorphism`
  - `cargo test -p deepseek-tui config::tests::instructions_paths --lib`
  - `cargo test -p deepseek-tui tools::subagent::tests::resident_file --lib`
  - `cargo test -p deepseek-tui core::engine::tests::build_tool_context_wires_lsp --lib`
  - `cargo test -p deepseek-tui --lib capacity_escalation`
  - `cargo test -p deepseek-tui --test protocol_recovery`
  - `cargo test -p deepseek-tui --lib sidecar_contract_full_lifecycle`
- [ ] `cd crates/desktop/web-ui && npm run test:f3 && npm run build` green.
- [ ] CHANGELOG entry under `[Unreleased] § Added` referencing this ADR.
- [ ] `git diff --stat HEAD~..HEAD` shows ≤ 500 LOC net.

## 8. Open questions (answer before M1 lands)

1. **`HostBundle` aggregation:** prefer (a) one trait with many `fn host_for_*(&self) -> &dyn …`, (b) generic struct over many type params (`Engine<M: McpHost, L: LspHost, …>`), or (c) named-field struct of `dyn` traits. Recommend **(c)** — simpler borrowing, matches existing TurnLoopHost field-method style; no GAT/HKT pain.
2. **`SandboxPolicy` parametrization:** ApprovalDecision is already
   generic. Should `EngineHandle` be generic too, or specialize on
   `SandboxPolicy` core-side? Recommend specialization — `SandboxPolicy`
   gets moved into core in M3 along with `SandboxBackend` trait.
3. **Test ownership:** `crates/tui/src/core/engine/tests.rs` (2495) — at
   what point do we cleave it? Recommend **never**; keep it as a tui
   integration suite that consumes the re-exported `Engine` (since the
   fixtures rely on tui tools / mcp / lsp).
4. **`deepseek-topic-memory` in core deps:** add or keep as `TopicMemoryHost` trait? Recommend **trait** — keeps core dep graph slim, and topic memory has 0 callers inside the core engine logic (it's only injected via prompt assembly which already goes through host).
5. **M-series vs B / GAP cadence:** can M-PRs land while B-L1 CRAFT
   followups land? Recommend **yes** — both touch disjoint files when
   M-series stays inside the engine boundary; the freeze window is
   the **sidecar contract**, not "all engine files".

## 9. Decision required (this spike's exit)

Maintainer reviews and decides:

- [ ] **Approve M0 spike output** → unblocks M1.
- [ ] Approve the seven-PR strangler plan in §6 as the structure (specific PR boundaries can shift on contact with reality, but cap and acceptance stays).
- [ ] Approve §7 M1 file scope as the first step.
- [ ] Approve the trait list in §2.2: `McpHost`, `LspHost`, `SandboxHost`, `SubAgentHost`, `ShellHost`, `SeamHost`, `CycleHost`, `WorkshopHost`, `TopicMemoryHost`. (Names are negotiable; count is not.)

## 10. Related docs

| Doc | Relation |
|-----|----------|
| [BACKLOG_ENGINE_STRUCT_IN_CORE.md](./BACKLOG_ENGINE_STRUCT_IN_CORE.md) | Parent ADR — promote to "in spike" once this lands. |
| [BACKLOG_RUNTIME_UNIFICATION.md](./BACKLOG_RUNTIME_UNIFICATION.md) | Sibling backlog — M-series **does not** route Zagens HTTP through `core::Runtime`. |
| [P2_MIGRATION_SPIKE.md](./P2_MIGRATION_SPIKE.md) | Original P2 PR0 spike (2026-05-22) — same structural pattern; this doc is its successor for the struct migration. |
| [P2_PR6_TURN_LOOP_L2_MIGRATION_PLAN.md](./P2_PR6_TURN_LOOP_L2_MIGRATION_PLAN.md) | Predecessor — moved turn_loop logic. M-series builds on its `TurnLoopHost`. |
| [P2_G3_ENGINE_L2_SIGNOFF.md](./P2_G3_ENGINE_L2_SIGNOFF.md) | Signoff that established "Engine struct stays in tui" — M-series is the explicit reversal once trait surface is ready. |
| [IMPLEMENTATION_SUMMARY_2026-05-24.md](./IMPLEMENTATION_SUMMARY_2026-05-24.md) §6 | Regression command source-of-truth. |
| [RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md) §11.0, §13.1, §17.5 | Boundary policy + risk register. |
