# Zagens documentation (public)

Technical docs for **contributors, integrators, and release maintainers**. End-user guides live on the website (see below).

| Audience | Location |
|----------|----------|
| End users | [zagens.com/docs](https://zagens.com/docs) — source in the website repo `content/docs/` |
| Maintainer-only notes | Local `doc_Private/` (`.gitignore` — not in this repo) |

---

## Getting started

| Doc | Description |
|-----|-------------|
| [LOCAL_DEV_VERIFY.md](./LOCAL_DEV_VERIFY.md) | Local lint, tests, pre-push hooks |
| [REPO_SPLIT.md](./REPO_SPLIT.md) | Product repo vs website repo |
| [user/README.md](./user/README.md) | User documentation pointer |

---

## Architecture & runtime

| Doc | Description |
|-----|-------------|
| [tech/RUNTIME_ARCHITECTURE.md](./tech/RUNTIME_ARCHITECTURE.md) | Runtime + desktop boundaries |
| [tech/API_DESIGN.md](./tech/API_DESIGN.md) | HTTP/SSE API design |
| [tech/openapi/zagens-runtime-v1.openapi.json](./tech/openapi/zagens-runtime-v1.openapi.json) | OpenAPI contract (CI drift check) |
| [tech/PERSISTENCE.md](./tech/PERSISTENCE.md) | Persistence model |
| [tech/SANDBOX_CAPABILITY_MATRIX.md](./tech/SANDBOX_CAPABILITY_MATRIX.md) | Sandbox capability matrix |
| [tech/TOOLS_PRINCIPLES.md](./tech/TOOLS_PRINCIPLES.md) | Tool design principles |
| [tech/KV_CACHE_OBSERVABILITY.md](./tech/KV_CACHE_OBSERVABILITY.md) | KV cache observability |

### ADRs (public subset)

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
| [tech/adr/G2_GATE_ACCEPTANCE.md](./tech/adr/G2_GATE_ACCEPTANCE.md) | G2 gate acceptance |
| [tech/adr/B2_INJECTION_ARBITRATION.md](./tech/adr/B2_INJECTION_ARBITRATION.md) | Injection arbitration |
| [tech/adr/A2_A3_SIGNOFF.md](./tech/adr/A2_A3_SIGNOFF.md) | A2/A3 sign-off |

---

## Desktop

| Doc | Description |
|-----|-------------|
| [desktop/VERSIONING.md](./desktop/VERSIONING.md) | Versioning policy |
| [desktop/UPDATER.md](./desktop/UPDATER.md) | OTA updates & signing |
| [desktop/I18N_PLAN.md](./desktop/I18N_PLAN.md) | Internationalization |
| [desktop/PREVIEW_ARCHITECTURE.md](./desktop/PREVIEW_ARCHITECTURE.md) | Preview panel architecture |
| [desktop/OFFICE_SCENARIOS.md](./desktop/OFFICE_SCENARIOS.md) | Office scenarios |
| [desktop/SMARTSCREEN.md](./desktop/SMARTSCREEN.md) | Windows SmartScreen notes |

---

## Harness & evaluation

| Doc | Description |
|-----|-------------|
| [harness/README.md](./harness/README.md) | Harness doc index |
| [harness/LHT_TEST_SUITE.md](./harness/LHT_TEST_SUITE.md) | Long-horizon regression suite |
| [harness/LHT_EVAL_INFRASTRUCTURE.md](./harness/LHT_EVAL_INFRASTRUCTURE.md) | L2 eval infrastructure |
| [harness/LONG_HORIZON_CODE_TASKS.md](./harness/LONG_HORIZON_CODE_TASKS.md) | Long-horizon code task spec |
| [harness/COMPOSABLE_HARNESS.md](./harness/COMPOSABLE_HARNESS.md) | Composable harness overview |
| [harness/fixtures/](./harness/fixtures/) | Evaluation fixtures |
| [harness/test-cases/](./harness/test-cases/) | Test case specs |

---

## Prompts & task types

| Doc | Description |
|-----|-------------|
| [task-type-prompt-architecture.md](./task-type-prompt-architecture.md) | Code / Office task types |
| [prompt-architecture.md](./prompt-architecture.md) | Prompt layering |
| [prompt-hallucination-patch.md](./prompt-hallucination-patch.md) | Hallucination guardrails |
| [craft-v2-improvements.md](./craft-v2-improvements.md) | CRAFT multi-agent |

---

## Skills (doc copies)

| Path | Description |
|------|-------------|
| [skill/multi-search-engine/](./skill/multi-search-engine/) | Multi-search-engine skill (runtime copy: `crates/runtime-server/assets/skills/`) |

---

## Moved out of `docs/` (private)

Session handoffs, implementation plans, paper drafts, symbol-index iteration notes, topic-memory reference tree, and similar material → local **`doc_Private/docs/`**. Migration script: `scripts/ci/move-docs-to-private.ps1`.
