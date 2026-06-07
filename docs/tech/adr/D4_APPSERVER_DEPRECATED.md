# D4 Decision — `app-server` Experimental Stack Marked Deprecated

**Status:** Removed (2026-05-26, D7 C5) — was deprecated 2026-05-26  
**Supersedes:** maintainer: `doc_Private/docs/tech/RUNTIME_EVOLUTION_ROADMAP.md` §4.2 "D4 freeze app-server" (2026-05-21) — freeze upgraded to **deprecated**  
**Related:** maintainer: `doc_Private/docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md` §1 #6 · [RUNTIME_ARCHITECTURE.md](../RUNTIME_ARCHITECTURE.md) · maintainer: `doc_Private/docs/tech/RUNTIME_EVOLUTION_ROADMAP.md`

## Background

The repository long had two HTTP runtime paths:

| Path | Purpose |
|------|---------|
| **Production** | **`deepseek-runtime`** → `runtime_api` (`/v1/*`) → `Engine` |
| **Experimental (removed)** | ~~`deepseek app-server`~~ → ~~`crates/app-server`~~ → ~~`deepseek-state`~~ |

Zagens / Desktop **only** uses the production path. The experimental path did not interoperate with sidecar persistence, auth, or SSE contracts, causing cognitive and maintenance cost (maintainer: `doc_Private/docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md` §3.9).

M-series (D5) is closed; the next structural priority is **D6 `runtime-server`** (slim binary extracted from the sidecar lineage), **not** promoting `app-server`.

## Decision

**`crates/app-server` and the `deepseek app-server` CLI subcommand are marked deprecated.**

- **Not promoted** as a second official HTTP runtime.
- **No code deletion in this phase** — ~~crate, CLI entry, dependencies retained~~ **removed (D7 C5)**.
- **`deepseek-state` (`crates/state`) not wholesale deprecated yet** — CLI `thread` etc. may still read/write `StateStore`; migrate or shrink scope when D7 persistence is unified.

## Production Path

- Zagens / Desktop: **`deepseek-runtime`** sidecar + `runtime_api` (`crates/runtime-server`)
- Headless / CI: same binary, HTTP + Bearer
- ~~`deepseek-tui`~~, ~~`deepseek` CLI~~: **removed in D6 Phase B** (see [D6_PHASE_B_CLI_SUNSET.md](./D6_PHASE_B_CLI_SUNSET.md))

## Execution (2026-05-26)

| Item | Action |
|------|--------|
| Docs | This ADR; maintainer: `doc_Private/docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md` D4 ✅, §1 #6 checked |
| CLI | `deepseek app-server` help labeled DEPRECATED |
| Crate | `deepseek-app-server` crate / `run` / `run_stdio` docs + `#[deprecated]` |
| Prohibited | No new app-server endpoints, no turn/API extensions, no desktop dependency |

## Subsequent Removal (D7 C5 ✅)

1. ~~Confirm no external scripts depend on `deepseek app-server`~~
2. ~~Delete `crates/app-server`, CLI subcommand, workspace dependency~~ — **done 2026-05-26**
3. `StateStore` shrunk to CLI legacy (`thread list --source state`); production list defaults to `runtime.db`

## Acceptance

- [x] Written decision (this file)
- [x] maintainer: `doc_Private/docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md` §1 #6 can be checked
- [x] Code physically removed (D7 C5, 2026-05-26)
