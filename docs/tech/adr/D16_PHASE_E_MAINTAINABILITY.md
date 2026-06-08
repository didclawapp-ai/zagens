# D16 — Phase E Maintainability Split Plan (Non-blocking Release)

> **Type:** Implementation plan (maintainability / engineering efficiency)  
> **Status:** **Closed (Checkpoint)** (2026-05-27) — see [D17_ARCHITECTURE_FREEZE.md](./D17_ARCHITECTURE_FREEZE.md) §3  
> **Prerequisites (Landed):** [D15_FINAL_ARCHITECTURE_CONVERGENCE.md](./D15_FINAL_ARCHITECTURE_CONVERGENCE.md)  
> **SSOT architecture diagram:** [RUNTIME_ARCHITECTURE.md](../RUNTIME_ARCHITECTURE.md)  
> **Relation to D15:** D15 establishes Desktop-only SSOT; **D16 does not change architecture invariants**, only splits oversized modules for maintainability  
> **⚠ No longer executing:** E1 phase 2 (full tools package migration), E1-d <500 line KPI, E4 client.ts split — cutoff details in D17 §3–§4

---

## 0. Boundaries

### 0.1 Problems to Solve

After D15 complete, architecture SSOT is established, but these **monolithic files / single crates** still affect iteration efficiency:

| Hotspot | Size (~) | Impact |
|---------|----------|--------|
| `crates/runtime-server` | ~100k lines Rust | Slow compile, coupled responsibilities, HTTP changes easily touch tools/MCP |
| `tools/subagent/mod.rs` | ~4000 lines | CRAFT multi-Agent changes concentrated, hard review |
| `web-ui/src/App.tsx` | ~2566 lines | Connection / session / SSE / approval / Agent state all in root component |

### 0.2 What D16 Does / Does Not Do

| Do | Do not |
|----|--------|
| Physical split crate / module / hooks | Change `/v1/*` HTTP contract |
| File size down to ~1000 line soft cap | Change Turn execution path (still via `RuntimeThreadManager` → `core::Engine`) |
| Tests migrate with modules | Desktop embed runtime (remove HTTP hop) |
| Optional OpenAPI contract test in CI (E5) | metrics / multi-sidecar / Capability Manifest (old D11–D14) |

### 0.3 Recommended Priority

| Order | Item | Rationale |
|-------|------|-----------|
| **E2** | SubAgent `mod.rs` file split | Single 4000-line file, clear scope, zero conflict with D15 |
| **E3** | `App.tsx` state machine extraction | High-frequency desktop feature iteration |
| **E1** | Split `runtime-server` crate | Large dependency chain change, suited to stable period |
| **E4–E5** | `client.ts` split, OpenAPI CI | Can parallel E3 |

---

## 1. E1 — Split `runtime-server` Large Crate

### 1.1 Current State

**Single crate:** `crates/runtime-server/` (package `zagens-cli`, lib `deepseek_runtime`, bin `deepseek-runtime`)

`src/lib.rs` registers 60+ top-level modules, including but not limited to:

| Module dir / file | Responsibility |
|-------------------|----------------|
| `runtime_api/` | Axum HTTP/SSE, `/v1/*` routing |
| `runtime_serve.rs` | Sidecar CLI entry |
| `runtime_threads/` | Thread/Turn orchestration, event broadcast |
| `core/engine/` | Engine adapter layer (Host injection) |
| `tools/` | All tool implementations (shell, file, subagent…) |
| `mcp/` | MCP connection pool |
| `session_manager.rs` + `session_store_sqlite.rs` | Session projection |
| `thread_store_sqlite.rs` + `runtime_threads/persist.rs` | Thread/Event SSOT |
| `task_manager.rs` / `automation_manager.rs` | Background tasks |
| `llm_client/` / `client/` | LLM calls |
| `compaction/` / `prompts.rs` / `symbol_index.rs` | Context and indexing |

**Production hotspots (>1000 lines, 2026-05-26 stats):**

| Lines | Path |
|-------|------|
| 4003 | `src/tools/subagent/mod.rs` |
| 2371 | `src/tools/shell.rs` |
| 1991 | `src/tools/file.rs` |
| 1754 | `src/task_manager.rs` |
| 1638 | `src/tools/web_run.rs` |
| 1531 | `src/skills/install.rs` |
| 1494 | `src/client/chat.rs` |
| 1417 | `src/session_manager.rs` |
| 1324 | `src/tools/apply_patch.rs` |
| 1290 | `src/tools/office_write.rs` |
| 1276 | `src/tools/registry.rs` |
| 1248 | `src/command_safety.rs` |
| 1182 | `src/symbol_index.rs` |
| 1824 | `src/localization.rs` |

> Test files (e.g. `runtime_threads/tests.rs` ~2500 lines) are large but test code; migrate with corresponding module on crate split, not listed separately as production debt.

### 1.2 Target Crate Layout

```text
crates/
├── runtime-api/              # package: deepseek-runtime-api
│   └── HTTP/SSE thin gateway: auth, router, openapi, stream
│
├── runtime-orchestrator/     # package: deepseek-runtime-orchestrator
│   └── RuntimeThreadManager, task/automation wiring, approval suspend bridge
│
├── runtime-adapters/         # package: deepseek-runtime-adapters
│   └── tools/, mcp/, persist, session, llm_client, compaction…
│
└── runtime-server/           # package: zagens-cli (thin shell)
    ├── lib: re-export + DI assembly only (target <500 lines)
    └── bin: deepseek-runtime (main unchanged)
```

### 1.3 Dependency Direction (Mandatory)

```text
runtime-server
  → runtime-api, runtime-orchestrator, runtime-adapters

runtime-api
  → runtime-orchestrator, deepseek-protocol

runtime-orchestrator
  → deepseek-core, runtime-adapters, deepseek-protocol

runtime-adapters
  → deepseek-core, deepseek-tools, deepseek-mcp, deepseek-config, …

deepseek-core
  → protocol, config (does not depend on runtime-api / adapters)
```

**Red lines (consistent with D15):**

- `deepseek-desktop` **still does not** path-depend any runtime crate  
- `/v1/*` paths and OpenAPI **unchanged**  
- `export-runtime-openapi` diff should be empty before/after crate split (unless contract intentionally changed)

### 1.4 Recommended PR Chain (E1)

| PR | Content | Exit criteria |
|----|---------|---------------|
| E1-a | Create `runtime-adapters`, migrate `tools/` + `mcp/` + persist related | `runtime-server` calls via adapters; tests green |
| E1-b | Create `runtime-orchestrator`, migrate `runtime_threads/` + `task_manager` | Turn single path unchanged |
| E1-c | Create `runtime-api`, migrate `runtime_api/` + `runtime_serve` routing layer | OpenAPI export no diff |
| E1-d | Slim `runtime-server` to DI + main | `lib.rs` <500 lines |

**Effort estimate:** 3–5 weeks (can parallel product features, progressive PR merge).

---

## 2. E2 — SubAgent `mod.rs` File Split

### 2.1 Current State

```
crates/runtime-server/src/tools/subagent/
├── mod.rs          ← ~4003 lines (problem)
├── blackboard.rs   ← existing
├── craft.rs        ← existing
├── mailbox.rs      ← existing
└── tests.rs        ← ~1521 lines
```

`mod.rs` mixes:

| Block | Representative types / functions |
|-------|----------------------------------|
| Runtime core | `SubAgentRuntime`, `SubAgentManager`, `SubAgent` |
| Tool impl (7+) | `AgentSpawnTool`, `AgentWaitTool`, `DelegateToAgentTool`, `AgentListTool`… |
| Execution logic | `run_subagent()`, `run_subagent_task()`, `wait_for_agents()` |
| Parse / route | `parse_spawn_request()`, `subagent_flash_router()` |
| Persistence | `PersistedSubAgent`, `write_json_atomic()`, `default_state_path()` |
| Prompt / tool surface | `subagent_system_prompt()`, `subagent_allowed_tools()` |

### 2.2 Target Directory

```text
subagent/
├── mod.rs              # re-export + ToolRegistry registration (target <300 lines)
├── runtime.rs          # SubAgentRuntime, SubAgentManager, SubAgent
├── spawn.rs            # AgentSpawnTool, parse_spawn_request
├── wait.rs             # AgentWaitTool, wait_for_result, wait_for_agents
├── delegate.rs         # DelegateToAgentTool
├── list_close.rs       # AgentListTool, AgentCloseTool, AgentCancelTool
├── assign_send.rs      # AgentAssignTool, AgentSendInputTool, AgentResumeTool
├── result.rs           # AgentResultTool
├── persist.rs          # PersistedSubAgent*, write_json_atomic, default_state_path
├── prompts.rs          # subagent_system_prompt, subagent_allowed_tools, build_subagent_system_prompt
├── router.rs           # subagent_flash_router, fallback_subagent_assignment_route
├── blackboard.rs       # keep
├── craft.rs            # keep
├── mailbox.rs          # keep
└── tests.rs            # keep (can add #[path] or mod tests subdir per new module)
```

### 2.3 Split Principles

1. **Each `*Tool` struct + `ToolHandler` impl in its own file** (or grouped by spawn/wait/delegate)  
2. **`mod.rs` only:** `pub mod` declarations, `pub use`, register with `ToolRegistryBuilder`  
3. **Shared types** (`SubAgentCompletion`, `SpawnRequest`, etc.) in `types.rs` or stay in `runtime.rs`  
4. **Single-file soft cap ~800 lines**; split again if exceeded  

### 2.4 Recommended PR Chain (E2)

| PR | Content |
|----|---------|
| E2-a | Extract `runtime.rs` + `persist.rs` + `prompts.rs` (no behavior change) |
| E2-b | Extract each Tool file (spawn / wait / delegate / list_close / assign_send / result) |
| E2-c | Extract `router.rs`; slim `mod.rs` to <300 lines |

**Verification:** `cargo test -p zagens-cli subagent` all green; CRAFT blackboard / AgentPanel manual smoke.

**Effort estimate:** 1–2 weeks.

---

## 3. E3 — Web UI `App.tsx` State Machine Extraction

### 3.1 Current State

**File:** `crates/desktop/web-ui/src/App.tsx` (~2566 lines)

Root component `App()` directly imports **40+** API / lib functions and holds **50+** `useState` / `useRef`, responsibilities include:

| Domain | Typical state / ref | Current responsibility |
|--------|-------------------|------------------------|
| **Runtime connection** | `runtimeConn`, `runtimeSessionEstablished`, `retryConnectRef`, `runtimeProbeFailStreakRef` | `initRuntimeConfig`, Sidecar ready, `waitForRuntimeReady`, reconnect |
| **Session / thread** | `sessions`, `activeSessionId`, `resumedThreadId`, `messages` | Sidebar list, switch session, `resumeSessionThread`, message cache |
| **SSE streaming Turn** | `streamingThreadIds`, `streamControllersRef`, `threadTurnRef`, `streamSessionRef` | `postStreamTurn` / poll SSE, `stopThreadTurn`, multi-window `filterThreadStreamEvents` |
| **Approval** | `approval`, `approvalBusy` | `ApprovalDialog`, `postResolveApproval` |
| **Agent / CRAFT** | `agentStates`, `pendingSpawnMetaRef` | SubAgent panel, spawn metadata |
| **Composer prefs** | `selectedModel`, `runMode`, `taskTypePreference`, `selectedWorkspace`… | Model / mode / workspace / routing intent |
| **UI layout** | `sidebarCollapsed`, `activeInspector`, `panelPreview` | Panel collapse, preview, Inspector |

**Related files (optional later E4):**

| File | Lines | Notes |
|------|-------|-------|
| `api/client.ts` | ~1330 | All REST/SSE client logic |
| `components/Composer.tsx` | ~1380 | Input and send |

### 3.2 Target Structure

```text
web-ui/src/
├── App.tsx                    # target <800 lines: layout + hooks composition
├── hooks/
│   ├── useRuntimeConnection.ts   # Sidecar port, ready, probe, reconnect
│   ├── useTurnSession.ts         # sessions, activeSession, messages, resume/switch
│   ├── useTurnStream.ts          # SSE subscribe, streaming, stop, event normalize
│   ├── useTurnApproval.ts        # approval suspend / resolve / busy
│   └── useAgentPanelState.ts     # agentStates, spawn meta, inspector linkage
├── api/                       # optional E4
│   ├── sessions.ts
│   ├── threads.ts
│   ├── stream.ts
│   └── client.ts              # thin re-export or deprecated forward
└── components/                # unchanged; App no longer calls 40+ client functions directly
```

### 3.3 Hook Responsibilities

#### `useRuntimeConnection`

- Encapsulates: `initRuntimeConfig`, `waitForRuntimeBootReady`, `probeRuntimeConnection`, `invalidateRuntimeBootReadyCache`
- Outputs: `runtimeConn`, `retryConnect()`, `runtimeSessionEstablished`
- Consumers: App shell, Sidebar refresh sessions timing

#### `useTurnSession`

- Encapsulates: `getSessions`, `getSessionDetail`, `resumeSessionThread`, `deleteSession`, `persistThreadSession`
- Outputs: `sessions`, `activeSessionId`, `messages`, `selectSession()`, `refreshSessions()`
- Works with `sessionUiCache`, `rebuildMessagesFromThreadEvents`

#### `useTurnStream`

- Encapsulates: `postStreamTurn`, `pollThreadTurnEvents`, `startThreadTurn`, `stopThreadTurn`, `filterThreadStreamEvents`
- Outputs: `streaming`, `sendMessage()`, `stopTurn()`, `threadTurnRef`
- Holds: `streamControllersRef`, `streamingThreadIds`

#### `useTurnApproval`

- Encapsulates: `postResolveApproval`, approval policy ref
- Outputs: `approval`, `approvalBusy`, `resolveApproval()`, `clearApproval()`

#### `useAgentPanelState`

- Encapsulates: `agentStates`, `pendingSpawnMetaRef`, with `useInspectorUnread` / CRAFT blackboard notifications
- Outputs: Agent panel state and upsert helpers

### 3.4 Recommended PR Chain (E3)

| PR | Content |
|----|---------|
| E3-a | Extract `useRuntimeConnection`; App uses hook |
| E3-b | Extract `useTurnSession` + `useTurnStream` (tightly coupled, can same PR) |
| E3-c | Extract `useTurnApproval` + `useAgentPanelState` |
| E3-d | App.tsx slim acceptance (<800 lines) |

**Verification:**

```bash
cd crates/desktop/web-ui && npm run build
```

Manual: cold start → send message → stream → Stop → approval → switch session → session list recovers after Sidecar restart.

**Effort estimate:** 2–3 weeks.

---

## 4. E4 / E5 — Optional Follow-up

### E4 — Split `api/client.ts`

Split by domain; `client.ts` keeps re-export to avoid large import churn:

| New file | Responsibility |
|----------|----------------|
| `api/sessions.ts` | `getSessions`, `getSessionDetail`, `resumeSessionThread`, `deleteSession` |
| `api/threads.ts` | `getThreadDetail`, `patchThread`, `startThreadTurn`, `forkThreadAtUserMessage`… |
| `api/stream.ts` | `postStreamTurn`, `pollThreadTurnEvents`, `filterThreadStreamEvents` |
| `api/runtimeBoot.ts` | `initRuntimeConfig`, `waitForRuntimeReady`, `probeRuntimeConnection` |

### E5 — OpenAPI Contract Test (CI)

| Step | Description |
|------|-------------|
| CI job | `export-runtime-openapi` → diff checked-in `zagens-runtime-v1.openapi.json` |
| Smoke | create thread → start turn → SSE events → resolve approval (can reuse existing test helper) |
| TS | `npm run generate:api-types` matches checked-in `runtime-api.ts` |

---

## 5. Definition of Done (D16 DoD — Checkpoint Closure)

**Signed off 2026-05-27:** D16 closes as **Checkpoint**, **not** waiting for all E1 Landed. Execution cutoff see [D17 §3](./D17_ARCHITECTURE_FREEZE.md).

| Sub-item | Acceptance | Status |
|----------|------------|--------|
| **E2** | `subagent/mod.rs` ~82 lines; 108 tests | ✅ Landed |
| **E3** | `AppShell` + hooks; `App.tsx` <800 lines | ✅ Landed |
| **E5** | OpenAPI + TS diff gate; golden path CI | ✅ Landed |
| **E1 phase 1** | api / orchestrator / adapters skeleton; §1.3 dependency direction; OpenAPI no unexpected diff | ✅ Landed |
| **E1 phase 2** | Full tools migration to adapters; `lib.rs` <500 lines | ⏸ **Paused** (D17) |
| **E4** | `client.ts` domain split | ❌ **Won't Do (v1)** |

**Not required:** E1 phase 2 / E4 complete to release Zagens — **Architecture Freeze v1 effective** (D17).

---

## 6. Roadmap Cross-reference

| ID | Topic | Relation to D16 |
|----|-------|---------------|
| D11 | Prometheus metrics | Optional after D16 complete |
| D12 | Per-workspace sidecar | **Not** in D16; needs separate ADR |
| D13 | Capability Manifest | Optional after D16 complete |
| D14 | MCP pool stability | Can overlap E1-adapters partially |

---

## 7. Recommended New-session Opener

**D16 is Closed (Checkpoint).** New sessions read [D17_ARCHITECTURE_FREEZE.md](./D17_ARCHITECTURE_FREEZE.md); **do not** continue E1 phase 2 unless a new ADR unfreezes.

```text
Architecture is Freeze v1. Read docs/tech/adr/D17_ARCHITECTURE_FREEZE.md first.
Product feature iteration primary; obey maintainer: `doc_Private/docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md` §7.1 red lines. Do not restart D16 E1 deep split.
```

---

*Maintenance: After sub-items Landed, check in this doc §5, and append link in [RUNTIME_ARCHITECTURE.md](../RUNTIME_ARCHITECTURE.md) §10.*
