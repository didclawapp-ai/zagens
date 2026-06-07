# D6 Phase B — Option B: Single Runtime Crate + CLI/TUI Sunset

> **Status:** Landed (2026-05-26)  
> **Supersedes:** `agent-host` fork path in maintainer: `doc_Private/docs/tech/adr/D6_PHASE_B_SPIKE.md` (replaced by single-crate merge)  
> **Related:** [D6_RUNTIME_SERVER.md](./D6_RUNTIME_SERVER.md) · [RUNTIME_ARCHITECTURE.md](../RUNTIME_ARCHITECTURE.md) · D6 implementation plan: maintainer: `doc_Private/docs/tech/adr/D6_IMPLEMENTATION_PLAN.md`

---

## 0. Decision

**Before first public release, execute Option B:**

1. **Delete** `crates/cli` (`deepseek` dispatcher) and the ratatui full-screen TUI (`crates/tui/src/tui/`, `commands/`).
2. **Merge** runtime host code into **`crates/runtime-server`** (lib + bin `deepseek-runtime`).
3. **Delete** `crates/tui` crate (after content moves to `runtime-server`).
4. **Shrink** `deepseek-state` (CLI legacy; delete if zero references).
5. Zagens **unchanged**: still embeds `deepseek-runtime`; does not link runtime lib.

**No longer retained:** `deepseek-tui` binary, `deepseek serve --http` dev fallback (unified to `deepseek-runtime`), `delegate_to_tui` subprocess chain.

---

## 1. Motivation

| Factor | Explanation |
|--------|-------------|
| **After D6 Phase A+** | Sidecar is slim, but **442** headless `dead_code` warnings — same lib serves HTTP + TUI |
| **Product** | D12 Desktop-only; CLI/TUI not a user surface (DEV_NOTES D14 upgraded to **removal**) |
| **Unreleased** | No external script contract burden; suitable for one-time crate boundary change |
| **9s cold start** | Partly from sidecar size/init; merge + startup path optimization can run in parallel (§6) |

---

## 2. Target Architecture

```text
crates/runtime-server/          # package: deepseek-runtime-server
  lib: deepseek_runtime          # former tui lib minus TUI tree
  bin: deepseek-runtime          # existing

crates/core/                    # unchanged
crates/desktop/                 # unchanged (sidecar path/docs only)
crates/config, tools, mcp, …    # unchanged

Deleted:
  crates/cli/
  crates/tui/
  # crates/state/ — deepseek-core still references; retained per B3.1
```

**Dependency direction (acyclic):**

```text
desktop → config, secrets
runtime-server (lib) → core, tools, config, protocol, topic-memory, …
runtime-server (bin) → runtime-server (lib)
```

---

## 3. PR Chain (execute in order)

### B0 — Prep and CLI deletion ✅

| Step | Action |
|------|--------|
| B0.1 | This ADR + CHANGELOG ✅ |
| B0.2 | Remove `crates/cli` from workspace ✅ |
| B0.3 | Extract `transcript_isomorphism` ✅ |
| B0.4 | Tests use `deepseek_core::approval::ApprovalMode` ✅ |

### B1 — Strip TUI tree ✅

| Step | Action |
|------|--------|
| B1.1 | Delete `src/tui/`, `src/commands/`, `src/main.rs` ✅ |
| B1.2 | Delete `config_ui`, `palette`, `deepseek_theme`, ratatui/crossterm/arboard deps ✅ |
| B1.3 | Clean `lib.rs`; CLI helpers kept in `cli/{doctor,setup,pr_prompt}.rs` ✅ |
| B1.4 | Move `export-runtime-openapi` bin to `runtime-server` ✅ |

### B2 — Merge crate ✅

| Step | Action |
|------|--------|
| B2.1 | `runtime-server/Cargo.toml` merge former `tui` deps; add `[lib] name = deepseek_runtime` ✅ |
| B2.2 | `tui/src/*` → `runtime-server/src/` (incl. `assets/`, `tests/`) ✅ |
| B2.3 | bin calls `deepseek_runtime` lib ✅ |
| B2.4 | Delete `crates/tui/` ✅ |
| B2.5 | CI/scripts `-p deepseek-runtime-server`; `deepseek_tui` → `deepseek_runtime` (code paths) ✅ |

### B3 — Cleanup and acceptance ✅

| Step | Action |
|------|--------|
| B3.1 | Delete `deepseek-state` (if zero refs) → **retained**: `deepseek-core` still compiles against it; **not** sidecar SSOT ✅ |
| B3.2 | Update CI, OpenAPI scripts, docs; `sidecar.rs` may optionally detect legacy `deepseek-tui` on disk ✅ |
| B3.3 | Acceptance commands (§5); `RUSTFLAGS=-Dwarnings` build; Zagens smoke ✅ |

**Effort:** ~**2–3 weeks** (1 person); each PR keeps `cargo test -p deepseek-runtime-server` regressable.

---

## 4. Non-goals

- Do not change `/v1/*` HTTP contract semantics  
- Do not merge sidecar into Tauri process  
- Do not do P2 multi-sidecar in this phase  

---

## 5. Acceptance

```bash
cargo check --workspace
cargo test -p deepseek-runtime-server --lib sidecar_contract_full_lifecycle
cargo test -p deepseek-runtime-server --test sidecar_binary_contract
cargo tree -p deepseek-runtime-server -i ratatui    # no match
cargo tree -p deepseek-runtime-server -i crossterm  # no match
! test -d crates/cli
! test -d crates/tui
```

- [x] Workspace has no `crates/cli`, `crates/tui`  
- [x] `RUNTIME_ARCHITECTURE.md` describes only `deepseek-runtime` single lib  
- [x] Zagens `npm run bundle:prepare` + smoke pass  

---

## 6. 9s Cold Start (parallel optimization, not Phase B blocker)

**Assumption:** From icon click to interactive Web main UI ≈ 9s (maintainer manual measure, 2026-05-26).

Phase B **marginal** benefit: smaller sidecar binary, fewer unused code paths, reduced dead_code compile size.

**Likely larger contributors (need profiling):**

| Phase | Possible duration | Optimization direction |
|-------|-------------------|------------------------|
| Tauri/WebView2 process + WebView first paint | 2–4s | release build, asset compression, reduce first-screen JS |
| `initRuntimeConfig` etc. `get_runtime_port` | blocks until `DS_PICK_READY` | parallel sidecar spawn; skills install to background |
| Sidecar `RuntimeThreadManager::open` + SQLite | 0.5–2s | WAL, lazy open non-critical stores |
| React hydrate + first-screen API | 1–3s | skeleton screen, defer non-critical panels |

**Recommendation:** Run Phase B **in parallel** with startup optimization — after B, add `tracing` timestamps to `deepseek-runtime` (existing `[deepseek-runtime] bound … (+Duration)`), Desktop side records `DS_PICK_READY` → first-screen ready.

---

## 7. Risks

| Risk | Mitigation |
|------|------------|
| Large PR wide regression surface | Strict B0→B3 split; contract test each step |
| `history_isomorphism` coupled to TUI history | B0.3 extract Message-only module |
| Internal scripts depend on `deepseek` | Docs change to `deepseek-runtime` + curl; no released users |
| MIT upstream naming `deepseek-tui` | `NOTICE.md` / `third-party/` retain attribution |

---

## 8. DEV_NOTES Revisions

- **D14 CLI positioning** → **removed** (2026-05-26, this ADR)  
- **D13 Sidecar** → `deepseek-runtime` only; crate name `deepseek-runtime-server`  
- ratatui TUI → **deleted** (not freeze)
