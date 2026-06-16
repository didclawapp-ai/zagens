# D17 — System Architecture Closure and Freeze (Architecture Freeze v1)

> **Type:** Architecture sign-off ADR (closure / discipline, not new feature mainline)  
> **Status:** Accepted (2026-05-27)  
> **Prerequisites (Landed):** D6 · D7 · D8 · D15 · D16 Checkpoint · maintainer: `doc_Private/docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md` §1 = 10/10  
> **SSOT architecture diagram:** [RUNTIME_ARCHITECTURE.md](../RUNTIME_ARCHITECTURE.md)  
> **Excludes:** Harness engine separation (see [docs/harness/](../../harness/README.md) vision, **not in this ADR execution scope**)

---

## 0. TL;DR — Where Execution Stops

| Question | Answer |
|----------|--------|
| More major architecture changes? | **No.** This ADR declares **Architecture Freeze v1**; refactor mainline closed. |
| Continue D16? | **Do not continue E1 deep split.** D16 closes as **Checkpoint** (see §3). |
| What does this ADR execute? | **Only §5 optional closure PRs** (docs Landed; F1/F2 on demand, non-blocking). |
| What does this ADR explicitly not execute? | **§4 full table** — incl. E1 phase 2, E4, D11–D14, Harness separation, etc. |
| Post-freeze mainline? | **Product features**, obey maintainer: `doc_Private/docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md` §7.1 red lines. |

**Cutoff line (one sentence):** Assessment 10/10 + D15 legacy removal + D16 high-ROI items Landed + three-crate skeleton (api / orchestrator / adapters) = **architecture set**; `runtime-server` still ~100k lines, `client.ts`/`commands.rs` still monolithic = **accepted, no longer architecture KPI**.

---

## 1. Background

Before 2026-05-26 completed:

- **D6:** `deepseek-runtime` sidecar, no ratatui / CLI/TUI  
- **D7:** Thread/Event SSOT, Session as projection  
- **D8:** OpenAPI + TS generation + contract CI  
- **D15:** Deleted `deepseek-state`, `core::Runtime`  
- **D16 (partial):** SubAgent / App hooks / OpenAPI CI / three-crate skeleton  

Maintainer paused mid D16 E1 deep split; assessment found **continuing full package migration low ROI**. This ADR turns "architecture finalized" from fact into **team-executable cutoff rules**, avoiding infinite refactor.

---

## 2. Post-freeze Target Architecture (Final Form)

```text
┌─────────────────────────────────────────────────────────────┐
│  L3 — Zagens (sole user product)                             │
│  crates/desktop + web-ui                                     │
│  Tauri · sidecar supervisor · runtime_proxy · PTY · keys/files │
└──────────────────────────┬──────────────────────────────────┘
                           │ L2: localhost /v1/* + Bearer (Rust injected)
                           ▼
┌─────────────────────────────────────────────────────────────┐
│  L2 — deepseek-runtime (sidecar binary + HTTP gateway)       │
│  crates/runtime-server                                       │
│  runtime_api/* · runtime_serve · tools/* · task_manager …    │
└──────────────┬──────────────────────────────┬─────────────────┘
               ▼                              ▼
    runtime-orchestrator              runtime-adapters
    (Thread/Turn orchestration)       (MCP · persist · tool host ports)
               └──────────────┬───────────────┘
                              ▼
                    zagens-core (Engine · LiveTurnMachine · V3TurnHost)

Persistence SSOT (defaults; override via `DEEPSEEK_RUNTIME_DIR` / `DEEPSEEK_TASKS_DIR` etc., see orchestrator/task code):
  ~/.deepseek/tasks/runtime/runtime.db  ← Thread / Turn / Event
  ~/.deepseek/sessions/sessions.db      ← Session projection (runtime_thread_id)
                                      + kernel_events (Kernel V3 append-only log)
```

**Turn sole production path (immutable shell; Kernel V3 in-core, 2026-06-16):**

```text
HTTP handler
  → RuntimeThreadManager::start_turn (sidecar thin delegate)
  → runtime-orchestrator::turn_lifecycle::start_turn
  → TurnEnginePort::start_turn (zagens-core: `EngineHandle` → Op::SendMessage)
  → runtime-server: EnginePlatformExt::dispatch_op → handle_send_message (host glue)
  → zagens-core::handle_deepseek_turn
       → LiveTurnMachine + EffectInterpreter (see AGENT_KERNEL_V3.md)
```

In other words: **orchestration entry** always at `RuntimeThreadManager::start_turn`; **host L2 (sidecar `Engine`) cannot be bypassed** before entering core's turn loop. No second production path "direct to core turn". Kernel V3 replaced the imperative inner loop **inside** `zagens-core` without reopening the D17 crate split.

---

## 3. D16 Closure Status (Execution Cutoff Details)

### 3.1 ✅ Completed (Landed — no more similar work)

| Sub-item | Cutoff status | Acceptance anchor |
|----------|---------------|-------------------|
| **E2** SubAgent module split | **Landed** | `crates/runtime-server/src/tools/subagent/mod.rs` ~82 lines (impl); `zagens-core` side `subagent` types/re-export only (thin layer); 108 SubAgent-related unit tests |
| **E3** App.tsx hooks | **Landed** | `App.tsx` ~772 lines; `AppShell` + hooks |
| **E5** OpenAPI contract CI | **Landed** | `scripts/check-openapi-contract.{ps1,sh}`; CI Ubuntu: `export-runtime-openapi` + `generate:api-types` diff gate; `sidecar_contract_*` |
| **E1-a phase 1** adapters skeleton | **Landed** | `runtime-adapters`: MCP, persist, network_gate, tool host ports |
| **E1-b phase 1–5** orchestrator core | **Landed** | `runtime-orchestrator`: manager, monitor, engine_load, task_port |
| **E1-c phase 1–6** runtime-api wire | **Landed** | auth/health/cors, task wire, OpenAPI SSOT |
| **E1-d phase 1–2** DI assembly | **Landed** | `runtime_serve/http.rs`; `run_http_server` re-export |
| **E1 in-module splits** | **Landed** | shell/file/web_run/skills/task_manager subdir-ized |

### 3.2 ⏸ Stops Here (Checkpoint — explicitly no longer doing)

| Sub-item | Original D16 target | **Cutoff decision** | Resume condition |
|----------|----------------------|---------------------|------------------|
| **E1 phase 2** | Full `tools/` migration to `runtime-adapters` | **Paused** | Separate ADR: `ToolExecutionHost` + no circular deps |
| **E1-b phase 6+** | Full `task_manager/` migration to orchestrator | **Paused** | Same or quantify PR conflict bottleneck |
| **E1-d final state** | `runtime-server/lib.rs` <500 line DI shell | **Abandoned as KPI** | Not Freeze acceptance |
| **E4** | `client.ts` split by domain | **Won't Do (v1)** | Separate item when API client changes become main bottleneck |

### 3.3 D16 Overall Status

**Closed (Checkpoint)** — see [D16_PHASE_E_MAINTAINABILITY.md](./D16_PHASE_E_MAINTAINABILITY.md) §5.

---

## 4. This ADR and Beyond — Explicit Non-execution List

The following **are not part of Architecture Freeze v1** and **must not** be started citing "complete D16 / architecture closure":

| # | Topic | Notes |
|---|-------|-------|
| N1 | D16 E1 phase 2 (full tools migration) | ToolContext ↔ adapters circular dep unresolved |
| N2 | D16 E4 (client.ts split) | v1 accepts ~1470 line monolith |
| N3 | Slim `runtime-server` to <500 lines | Accept ~100k line sidecar lib |
| N4 | Split `desktop/commands.rs` | D1 signed off no split (~1.5k) |
| N5 | D11 Prometheus metrics | P2 backlog |
| N6 | D12 per-workspace sidecar | Needs separate ADR |
| N7 | D13 Capability Manifest | Harness compositional assembly, vision |
| N8 | D14 MCP pool stability | P2 backlog |
| N9 | Harness engine separate repo/crate | See docs/harness, **not this ADR** |
| N10 | Desktop embed runtime (remove HTTP hop) | Re-evaluate v0.7+ |
| N11 | Rewrite `/v1/*` or Engine turn path | Assessment §7.1 prohibited |
| N12 | Restore CLI/TUI / `deepseek-state` / `core::Runtime` | D15/D6 deleted |

**New session default assumption:** Architecture frozen; if PR touches N1–N12, architecture owner must explicitly sign off new ADR.

---

## 5. D17 Optional Closure PRs (Non-blocking, Execution Cap)

This ADR **doc sign-off counts as D17 Landed**. Following PRs **optional**; **skipping all does not affect Freeze announcement**:

| ID | Content | Execution cap | Skip consequence |
|----|---------|---------------|------------------|
| **F0** | This doc + related doc updates | **Done (this commit)** | — |
| **F1** | Stale `deepseek-tui` production comment cleanup | Only `core/engine/*`, `subagent`, `sandbox`, `config` comments; **no behavior change** | ✅ Landed (2026-05-27) |
| **F2** | `scripts/check-architecture-freeze.{ps1,sh}` | Optional: chain **`cargo test` hits** `architecture_invariants`, `architecture_boundary`, and your local OpenAPI check scripts; **no new gate rules** (this workspace mainline **has no ratatui dep**, do not add "ratatui tree" checks) | ✅ Landed (2026-05-27) |
| **F3** | Workspace untracked cleanup | D16-unrelated files only | None |

**If F1/F2 not merged by 2026-06-30, considered abandoned, no further tracking.**

---

## 6. Architecture Invariants (PR Review — Consistent with D15/D16)

Before merging any PR touching `desktop` / `runtime-server` / `core` boundaries:

| # | Rule |
|---|------|
| I1 | `desktop` **must not** path-depend `runtime-server` / `zagens-core` (**code review must check** `Cargo.toml`) |
| I2 | WebView **must not** hold runtime Bearer; via `runtime_proxy` |
| I3 | New `/v1/*` **must** OpenAPI + TS regen + contract test |
| I4 | **Must not** add fields to `Engine` struct |
| I5 | **Must not** introduce third persistence SSOT |
| I6 | Turn **must** go through `RuntimeThreadManager::start_turn` |
| I7 | Sidecar **`deepseek-runtime` / `runtime-server` dep graph must not introduce ratatui** (current workspace mainline has none; new deps need manual manifest review) |
| I8 | New `.rs`/`.tsx` >1000 lines → **prefer in-crate module split**; **do not** create new crate for this |

Existing CI / script guardrails:

- `crates/runtime-server/tests/architecture_invariants.rs` (D15: `deepseek_state` / legacy port etc.)
- `crates/desktop/tests/architecture_boundary.rs` (**automated**: forbid `../runtime-server`, `../core`, `zagens-core`, forbid `deepseek-tui` **lib** path-dep)
- OpenAPI: `scripts/check-openapi-contract.{ps1,sh}`; **CI** (`ci.yml` Ubuntu) also `git diff` gate on `zagens-runtime-v1.openapi.json` and `runtime-api.ts` (D8 / D16 E5)

---

## 7. Default Landing Points for New Features (Post-freeze)

| Need type | Landing point |
|-----------|---------------|
| HTTP / wire types | `runtime-api` |
| Thread/Turn lifecycle | `runtime-orchestrator` |
| MCP / persist / network policy | `runtime-adapters` |
| Tool impl / LLM / prompts | `runtime-server` (host) |
| Engine / turn logic | `zagens-core` + host trait |
| Desktop UI / OS integration | `desktop` + `web-ui` |

---

## 8. Onboarding Read Order

1. [RUNTIME_ARCHITECTURE.md](../RUNTIME_ARCHITECTURE.md)  
2. **This doc (D17)** — cutoff line and prohibited zone  
3. maintainer: `doc_Private/docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md` §7.1 — PR red lines  
4. [API_DESIGN.md](../API_DESIGN.md) + [OpenAPI JSON](../openapi/zagens-runtime-v1.openapi.json)  
5. [PERSISTENCE.md](../PERSISTENCE.md)  

**No need to read:** D6 implementation details (`doc_Private/`), D16 E1 phase 2 plan, SESSION_HANDOFF_D16 (archived to `doc_Private/`).

---

## 9. Definition of Done (D17 DoD)

| # | Acceptance item | Status |
|---|-----------------|--------|
| 1 | This doc Accepted | ✅ |
| 2 | D16 marked Closed (Checkpoint) | ✅ |
| 3 | RUNTIME_ARCHITECTURE §10 updated | ✅ |
| 4 | CHANGELOG records Freeze v1 | ✅ |
| 5 | Assessment §0 points to D17 | ✅ |
| 6 | F1/F2 optional PRs | ✅ Landed (2026-05-27) |

**Announcement (internal/external):**

> Zagens system architecture **Architecture Freeze v1** (2026-05-27) effective. Assessment 10/10; Sidecar + OpenAPI stable ABI; refactor mainline closed; product features can iterate normally.

---

## 10. Related Documents

| Document | Relation |
|----------|----------|
| [D15_FINAL_ARCHITECTURE_CONVERGENCE.md](./D15_FINAL_ARCHITECTURE_CONVERGENCE.md) | Legacy removal (Landed) |
| [D16_PHASE_E_MAINTAINABILITY.md](./D16_PHASE_E_MAINTAINABILITY.md) | Maintainability split (Closed Checkpoint) |
| maintainer: `doc_Private/docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md` | 10/10 verdict + §7.1 red lines |
| SESSION_HANDOFF_D16_PHASE_E | Archived at `doc_Private/docs/tech/adr/` |

---

*Maintenance: If Architecture Freeze v2 needed (e.g. Harness separation, Desktop embed runtime), new ADR must explicitly unfreeze and revise §4.*
