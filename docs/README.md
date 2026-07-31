# Zagens design specifications (`docs/`)

This directory contains **stable design specs** only — architecture, API contracts, feature specifications, and ADRs. **Language:** English (localized root READMEs: [中文](../README.zh-CN.md) · [日本語](../README.ja.md) · [Português (BR)](../README.pt-BR.md)).

| Audience | Location |
|----------|----------|
| End users | [zagens.com/docs](https://zagens.com/docs) |
| Contributing / local CI | [CONTRIBUTING.md](../CONTRIBUTING.md) · [LOCAL_DEV_VERIFY.md](../LOCAL_DEV_VERIFY.md) |
| Maintainer notes (not published) | Local `doc_Private/` |

Harness **fixtures** (TOML, demo data, oracle scripts) live under [`fixtures/harness/`](../fixtures/harness/) — executable assets, not prose docs.

---

## Architecture & API

| Doc | Description |
|-----|-------------|
| [tech/RUNTIME_ARCHITECTURE.md](./tech/RUNTIME_ARCHITECTURE.md) | Runtime + desktop boundaries |
| [tech/AGENT_KERNEL_V3.md](./tech/AGENT_KERNEL_V3.md) | Event-sourced turn engine (Kernel V3 — sole production path) |
| [tech/API_DESIGN.md](./tech/API_DESIGN.md) | HTTP/SSE API design |
| [tech/openapi/zagens-runtime-v1.openapi.json](./tech/openapi/zagens-runtime-v1.openapi.json) | OpenAPI contract (CI drift check) |
| [prompt-architecture-diagram.svg](./prompt-architecture-diagram.svg) | Prompt stack diagram |
| [tech/PERSISTENCE.md](./tech/PERSISTENCE.md) | Persistence model |
| [tech/SANDBOX_CAPABILITY_MATRIX.md](./tech/SANDBOX_CAPABILITY_MATRIX.md) | Sandbox capability matrix (Windows native sandbox implemented) |
| [tech/TOOLS_PRINCIPLES.md](./tech/TOOLS_PRINCIPLES.md) | Tool design principles; §9 evidence/intent/context-economy **shipped vs open** |
| [tech/KV_CACHE_OBSERVABILITY.md](./tech/KV_CACHE_OBSERVABILITY.md) | KV cache observability |

### ADRs (architecture decisions)

| Doc | Description |
|-----|-------------|
| [tech/adr/D6_RUNTIME_SERVER.md](./tech/adr/D6_RUNTIME_SERVER.md) | Runtime server split |
| [tech/adr/D6_PHASE_B_CLI_SUNSET.md](./tech/adr/D6_PHASE_B_CLI_SUNSET.md) | CLI sunset |
| [tech/adr/D8_OPENAPI_TS_GENERATION.md](./tech/adr/D8_OPENAPI_TS_GENERATION.md) | Web UI type generation |
| [tech/adr/D15_FINAL_ARCHITECTURE_CONVERGENCE.md](./tech/adr/D15_FINAL_ARCHITECTURE_CONVERGENCE.md) | Architecture convergence |
| [tech/adr/D16_PHASE_E_MAINTAINABILITY.md](./tech/adr/D16_PHASE_E_MAINTAINABILITY.md) | Maintainability phase E |
| [tech/adr/D17_ARCHITECTURE_FREEZE.md](./tech/adr/D17_ARCHITECTURE_FREEZE.md) | Architecture freeze |
| [tech/adr/D4_APPSERVER_DEPRECATED.md](./tech/adr/D4_APPSERVER_DEPRECATED.md) | AppServer deprecated |
| [tech/adr/D9_D10_DESKTOP_UX.md](./tech/adr/D9_D10_DESKTOP_UX.md) | Desktop UX |
| [tech/adr/RUNTIME_BASELINE.md](./tech/adr/RUNTIME_BASELINE.md) | Runtime baseline |
| [tech/adr/V2_API_VERSIONING.md](./tech/adr/V2_API_VERSIONING.md) | API versioning |
| [tech/adr/B2_INJECTION_ARBITRATION.md](./tech/adr/B2_INJECTION_ARBITRATION.md) | Injection arbitration |

---

## Desktop product design

| Doc | Description |
|-----|-------------|
| [desktop/PREVIEW_ARCHITECTURE.md](./desktop/PREVIEW_ARCHITECTURE.md) | Preview panel architecture |
| [desktop/MERMAID_PREVIEW_TOLERANCE.md](./desktop/MERMAID_PREVIEW_TOLERANCE.md) | Mermaid → WebView2 tolerance layer (markdown preview) |
| [desktop/OFFICE_SCENARIOS.md](./desktop/OFFICE_SCENARIOS.md) | Historical Office scenario map (built-in Office removed; use skill `zagens-office`) |
| [desktop/SCHEDULED_TASKS.md](./desktop/SCHEDULED_TASKS.md) | Scheduled tasks (RRULE automations → background Tasks) |
| [desktop/HOOKS.md](./desktop/HOOKS.md) | Lifecycle shell hooks (config, Cursor JSON, protocol) |
| [desktop/GITHUB_ACTION.md](./desktop/GITHUB_ACTION.md) | GitHub Actions · `coverage-gate` composite action |

---

## Harness & agents (feature specs)

| Doc | Description |
|-----|-------------|
| [harness/README.md](./harness/README.md) | Harness spec index |
| [harness/LONG_HORIZON_CODE_TASKS.md](./harness/LONG_HORIZON_CODE_TASKS.md) | Long-horizon code tasks |
| [harness/COMPOSABLE_HARNESS.md](./harness/COMPOSABLE_HARNESS.md) | Composable completion gates |
| [craft-v2-improvements.md](./craft-v2-improvements.md) | CRAFT multi-agent |
| [task-type-prompt-architecture.md](./task-type-prompt-architecture.md) | Task types (Code; Office removed) |
| [prompt-architecture.md](./prompt-architecture.md) | Prompt layering |

---

## Not in `docs/` (by design)

Release notes, eval runbooks, test-case lab notebooks, repo-split ops, and iteration plans → `doc_Private/docs/`. Migration: `scripts/ci/move-docs-to-private.ps1`, `scripts/ci/move-docs-spec-only.ps1`.
