# D15 — Final Architecture Convergence Plan (Desktop-only SSOT)

> **Type:** Implementation plan (convergence phase, not new feature mainline)  
> **Status:** Landed (2026-05-26)  
> **Prerequisites (Landed):** M1–M8 · D6 Phase B · D7 · D8 · D9/D10 · Zagens v0.5.0  
> **Product intent:** **Zagens Desktop** as sole user entry, replacing upstream deepseek-tui 0.8.15 TUI/CLI; Sidecar as embedded runtime, not a second product surface  
> **SSOT architecture diagram:** [RUNTIME_ARCHITECTURE.md](../RUNTIME_ARCHITECTURE.md)  
> **Relation to old numbering:** This **D15** means "architecture convergence finale"; **D11–D14** in maintainer: `doc_Private/docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md` (metrics / multi-sidecar / Capability Manifest / MCP pool) are **post-freeze enhancements**, not blocking this plan

---

## 0. Goals and Definition of Done

### 0.1 Problems to Solve

After multiple refactors (Sidecar validation → Engine into core → D6 TUI removal → D7 persistence linking), code still retains **TUI/CLI-era ghost paths** and **dual-track mental model**, causing:

- Newcomers still understand code as "TUI + Sidecar in parallel"  
- `deepseek-state` / `core::Runtime` have no production callers but occupy compile graph and cognition  
- Session and Thread are **linked** in D7, but API/storage still look like two primary datasets  
- `runtime-server` single crate ~100k lines, hindering long-term stable maintenance  

### 0.2 What D15 Does / Does Not Do

| Do | Do not |
|----|--------|
| Delete `state` crate and `core::lib.rs` legacy `Runtime` chain | Rewrite `/v1/*` HTTP contract |
| Confirm D7 persistence narrative as **Thread/Event SSOT**, Session demoted to projection | Per-workspace independent sidecar (old roadmap D12) |
| Unify comments/naming (TUI → Runtime adapter / Sidecar) | Prometheus metrics (old roadmap D11) |
| Add CI architecture gate to prevent legacy rebound | Capability Manifest merge (old roadmap D13) |
| Optional: split `runtime-server` into api / orchestrator / adapters | Desktop embed runtime (remove HTTP hop) — defer v0.7+ |
| Optional: Web UI `App.tsx` state machine extraction | Chase upstream TUI new features |

### 0.3 "Architecture Unified Complete" Acceptance (Definition of Done)

When **all** conditions below are met, announce internally/externally **Desktop has replaced TUI, architecture SSOT established**:

1. **User entry:** Docs and product describe only Zagens Desktop; no CLI/TUI user path  
2. **Zero legacy production refs:** workspace has no `deepseek-state`; `core` has no `Runtime` / `ThreadManager` / `JobManager` / `ThreadMessageTurnPort`  
3. **Single Turn path:** production code only `RuntimeThreadManager` → `core::Engine` → `EngineToolDispatch`  
4. **Persistence SSOT:** Thread + Event authoritative; Session API readable/writable but **does not independently grow primary data** (see §3)  
5. **Desktop boundary:** `desktop` does not path-depend `runtime-server` / `deepseek-tui` (existing tests, keep)  
6. **Contract:** OpenAPI export + at least one golden-path integration test (create thread → turn → SSE → approval)  
7. **Release:** CHANGELOG records D15; Zagens ≥ v0.5.x stable main flow  

---

## 1. Current Baseline (2026-05-26)

### 1.1 Completed (no redo)

| Milestone | Evidence |
|-----------|----------|
| TUI crate / ratatui deleted | D6 Phase B; `cargo tree -i ratatui` no match |
| Sidecar binary | `deepseek-runtime` HTTP only |
| Engine strangler | M1–M8; `core::engine::Engine` + Host traits |
| Desktop process isolation | `architecture_boundary.rs` |
| D7 linking | `sessions.db.runtime_thread_id` ↔ `runtime.db` |
| OpenAPI + TS | D8; `export-runtime-openapi` + `generate:api-types` |
| Product release | `release(zagens): v0.5.0` |

### 1.2 Pending Convergence (D15 scope)

| Item | Current state | Target |
|------|---------------|--------|
| `crates/state` | Only `core` depends; Sidecar comments mark non-SSOT | **Delete** |
| `core/src/lib.rs` | ~1839 lines; contains `Runtime`/`ThreadManager`/`JobManager` | **Delete legacy block**; keep re-export or split to `core/src/legacy/` then delete |
| `ThreadMessageTurnPort` | Only used in `runtime_threads/tests.rs` | **Delete** trait + `RuntimeThreadMessageTurnPort` |
| Session vs Thread | D7 linked; Sidebar still uses `/v1/sessions` | **Session = Thread projection** (§3) |
| Naming | Many `deepseek-tui` / `Tui-side` comments | **Batch replace** |
| `sidecar.rs` | `legacy_tui` binary branch | **Delete** |
| `runtime-server` size | ~103k lines Rust | **Phase E optional split** |

### 1.3 Architecture Invariants (for PR review)

After merge, every PR must obey:

1. New code **must not** depend on `deepseek-state`  
2. New Turn **must not** bypass `RuntimeThreadManager::start_turn`  
3. Desktop WebView **must not** hold runtime Bearer (keep Tauri proxy)  
4. New persistence **must not** introduce a third SSOT  
5. New `.rs`/`.tsx` default **≤1000 lines** (exceed requires ADR exemption)  

---

## 2. Target Architecture (Post-convergence)

```text
┌─────────────────────────────────────────────────────────────┐
│  Zagens (sole user product)                                  │
│  crates/desktop + web-ui                                     │
│  Tauri · Sidecar supervisor · runtime_proxy · PTY            │
└──────────────────────────┬──────────────────────────────────┘
                           │ localhost /v1/* + Bearer (Rust injected)
                           ▼
┌─────────────────────────────────────────────────────────────┐
│  deepseek-runtime (embedded subprocess, not user CLI)        │
│  ┌─────────────┐  ┌──────────────────┐  ┌───────────────┐ │
│  │ runtime-api │→ │ RuntimeThread    │→ │ core::Engine  │ │
│  │ (HTTP/SSE)  │  │ Manager          │  │ + turn_loop   │ │
│  └─────────────┘  └────────┬─────────┘  └───────┬───────┘ │
│                              │                    │         │
│                    RuntimeThreadStore      tools/mcp/llm    │
│                    (threads/turns/events)   (adapters)    │
└─────────────────────────────────────────────────────────────┘

Persistence SSOT:
  ~/.deepseek/tasks/runtime/runtime.db   ← Thread / Turn / Event
  ~/.deepseek/sessions/sessions.db       ← Session projection (incl. runtime_thread_id)

Deleted:
  deepseek-state · core::Runtime · CLI/TUI entry · ThreadMessageTurnPort
```

---

## 3. Phase Breakdown and PR Chain

Recommend **5 phases, 8–12 PRs**, total **4–8 weeks** (estimate 2–3 refactor PRs per week).  
**Rule: each PR mergeable independently; order within phase must not be reversed.**

---

### Phase A — Freeze and Guardrails (~3 days, 1 PR)

**PR-A1: `chore(arch): D15 invariants + CI grep gates`**

| Action | Detail |
|--------|--------|
| New test | `crates/runtime-server/tests/architecture_invariants.rs` |
| Gate 1 | `runtime-server` / `desktop` production code has no `deepseek_state` / `StateStore` |
| Gate 2 | `desktop/Cargo.toml` has no `runtime-server` / `deepseek-tui` path dep (extend existing boundary test) |
| Gate 3 | Optional: `scripts/check-architecture.sh` for CI |
| Docs | This doc Status → In Progress; CHANGELOG `[Unreleased]` add D15 entry |

**Exit criteria:** CI red blocks merge; legacy refs **allowed in core/state**, but **no new additions**.

---

### Phase B — Delete Legacy Orchestration Layer (~1–2 weeks, 2 PRs)

**PR-B1: `refactor(core): remove legacy Runtime and state dependency`**

| Delete/Modify | Path |
|---------------|------|
| Delete crate | `crates/state/` (incl. `parity_state` tests — migrate necessary assertions to runtime tests) |
| Major delete | `JobManager`, `ThreadManager`, `Runtime` and ~1500 lines serving only them in `crates/core/src/lib.rs` |
| Delete | `crates/core/src/thread_message_turn.rs` |
| Modify | `crates/core/Cargo.toml` — remove `deepseek-state` |
| Modify | Root `Cargo.toml` workspace members — remove `state` |
| Keep | `core/src/engine/*`, `protocol` re-export if needed, slim re-export from `lib.rs` |

**PR-B2: `refactor(runtime): drop ThreadMessageTurnPort shim`**

| Delete | `crates/runtime-server/src/runtime_threads/thread_message_turn_port.rs` |
| Modify | `runtime_threads/mod.rs` — remove `RuntimeThreadMessageTurnPort` export |
| Modify | `runtime_threads/tests.rs` — delete PR5 port tests; keep `start_turn` direct tests |

**Verification:**

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features
rg 'deepseek-state|StateStore|ThreadMessageTurnPort|core::Runtime' crates/ --glob '!**/target/**'
# Expected: 0 production matches
```

**Exit criteria:** workspace compiles; no `state` crate; grep zero production hits.

**Risk:** Hidden tests depend on `core::Runtime` — switch to `RuntimeThreadManager` fixture.  
**Rollback:** Single PR revert; no data migration.

---

### Phase C — Persistence SSOT Confirmation (~1–2 weeks, 2–3 PRs)

D7 already landed `runtime_thread_id` linking; this phase **confirms narrative matches write paths**.

**PR-C1: `docs+test(runtime): document Thread/Event as SSOT; session as projection`**

| Action | Detail |
|--------|--------|
| Update | `docs/tech/PERSISTENCE.md` — Thread/Event SSOT; Session projection table |
| Test | Golden path: `create_thread` → `persist-session` → `list_sessions` → `resume` reuses same thread |
| Audit | List all paths writing `SessionManager`; mark whether can change to write Thread then project |

**PR-C2: `refactor(runtime): session writes derive from thread (no orphan sessions)`**

| Action | Detail |
|--------|--------|
| Modify | `runtime_api/threads.rs` — sync session projection on create/update thread |
| Modify | `runtime_api/sessions.rs` — delete/resume only operate linked thread |
| Prohibit | Create session **without** `runtime_thread_id` (API validation) |
| Optional | One-time migration: scan orphan sessions → create thread or mark archived |

**PR-C3 (optional): `feat(runtime): session list reads thread store directly`**

| Action | Detail |
|--------|--------|
| Optimize | `list_sessions` prefer JOIN `runtime.db` metadata |
| Desktop | Confirm Sidebar consistent after sidecar restart (regress `b04864d`-class bug) |

**Exit criteria:**

- Every session must have `runtime_thread_id` (new data)  
- Event log can rebuild UI (existing `rebuildMessagesFromThreadEvents`)  
- No new code path that "writes session only, not thread"  

**Risk:** Old user orphan sessions — migration PR needs dry-run log.  
**Rollback:** Keep `runtime_thread_id` column; rollback write-path logic only.

---

### Phase D — Naming and Sidecar Cleanup (~1 week, 2 PRs)

**PR-D1: `refactor: TUI → Runtime adapter naming (comments + prompts)`**

| Scope | Example |
|-------|---------|
| `runtime-server/src/core/engine.rs` | "Tui-side" → "Runtime adapter" |
| `core/src/engine/mod.rs` | Update stale "live in deepseek-tui" comments |
| `prompts.rs` | Confirm `CLIENT_IDENTITY_DS_PICK` is Desktop SSOT; delete or gate `CLIENT_IDENTITY_TERMINAL` |
| `config/src/lib.rs` | "TUI-compatible" → "runtime-compatible config.toml" |

**PR-D2: `refactor(desktop): remove legacy_tui sidecar spawn paths`**

| Modify | `crates/desktop/src/sidecar.rs` |
|--------|--------------------------------|
| Delete | `legacy_tui` branch, `deepseek-tui` candidate binary |
| Keep | `deepseek-runtime` + bundled `binaries/deepseek-runtime-*` |
| Test | Single-path sidecar spawn integration test |

**Exit criteria:** `rg 'deepseek-tui|ratatui|Tui-side' crates/ --glob '*.rs'` only NOTICE/test fixture/historical ADR remain.

---

### Phase E — Maintainability (optional, v0.6+, 3–5 PRs)

**Does not block D15 DoD.** After architecture unified announcement, product features can run in parallel.  
**Detailed split plan:** [D16_PHASE_E_MAINTAINABILITY.md](./D16_PHASE_E_MAINTAINABILITY.md)

| PR | Content | Priority |
|----|---------|----------|
| E2 | Split `tools/subagent/mod.rs` (mailbox/craft/spawn/wait…) | **High** (recommended first) |
| E3 | Extract `useRuntimeConnection` + `useTurnSession` hooks from `web-ui/App.tsx` | Medium |
| E1 | Split `runtime-server` → `runtime-api` + `runtime-orchestrator` + `runtime-adapters` | Medium |
| E4 | Split `api/client.ts` by domain | Low |
| E5 | OpenAPI contract test in CI (export diff + smoke HTTP) | High (recommend early) |

---

## 4. PR Order Overview

```text
A1  CI gates + D15 doc
 │
 ├─ B1  delete state + core::Runtime
 ├─ B2  delete ThreadMessageTurnPort
 │
 ├─ C1  persistence SSOT docs + golden test
 ├─ C2  session projection writes
 ├─ C3  (opt) session list from thread store
 │
 ├─ D1  naming cleanup
 └─ D2  sidecar legacy removal
      │
      └─ E*  optional maintainability (parallel)
```

**Suggested merge strategy:** B1+B2 same week; C depends on B; D parallel with C; E1 depends on D15 DoD.

---

## 5. Verification Matrix

Run before each Phase merge:

| Command | Purpose |
|---------|---------|
| `cargo check --workspace` | Compile |
| `cargo test --workspace` | Unit + integration |
| `cargo clippy --workspace --all-targets --all-features` | Lint |
| `cargo tree -p deepseek-runtime-server -i ratatui` | No TUI |
| `.\scripts\export-runtime-openapi.ps1` | OpenAPI no unexpected diff |
| `cd crates/desktop/web-ui && npm run build` | TS strict |
| Manual | Zagens cold start → chat → tools → approval → restart sidecar → Sidebar consistent |

---

## 6. Risks and Mitigation

| Risk | Mitigation |
|------|------------|
| Miss test refs after deleting `state` | Full workspace `rg StateStore` before B1; CI gate |
| Session migration loses history | C2 migration dry-run + backup `sessions.db` |
| Large PR hard to review | Strict B1/B2 split; B1 only touches core/state |
| Refactor rebound | Phase A CI gate + PR template invariant check |
| Product release conflict | D15 finale PR can ship with Zagens v0.5.1 patch |

---

## 7. CHANGELOG Template

```markdown
### Architecture
- **D15:** Final architecture convergence — removed `deepseek-state` and legacy `core::Runtime`; Session is a projection of RuntimeThreadStore; Sidecar spawn paths unified to `deepseek-runtime` only. Desktop is the sole user entry (replaces upstream TUI/CLI).
```

---

## 8. Recommended New-session Opener

```text
Execute D15 architecture finale. Read docs/tech/adr/D15_FINAL_ARCHITECTURE_CONVERGENCE.md first.
Current phase: [A/B/C/D/E]. Start from PR-__. Do not commit/push unless I ask.
```

---

## 9. Relation to Subsequent Roadmap

| Old numbering (maintainer: `doc_Private/docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md` §5) | Relation to D15 |
|-------------------------------|-----------------|
| D11 metrics | Optional after D15 complete |
| D12 per-workspace sidecar | **Not** in Desktop-only finale scope; needs separate ADR |
| D13 Capability Manifest | After D15; Harness proposal already attached |
| D14 MCP pool stability | Can parallel Phase E |

---

*Maintenance: After D15 DoD all checked, change this doc Status to **Landed**, and remove corresponding items from RUNTIME_ARCHITECTURE.md § remaining debt.*
