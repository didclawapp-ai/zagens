# Harness design specifications

Public **feature specs** for long-horizon code tasks and composable completion gates.

| Doc | Description |
|-----|-------------|
| [LONG_HORIZON_CODE_TASKS.md](./LONG_HORIZON_CODE_TASKS.md) | Task graph, cycles, verification gates, LHT phases |
| [COMPOSABLE_HARNESS.md](./COMPOSABLE_HARNESS.md) | Layered completion gates (operator / model / toolchain) |
| [skill-manifest-schema.md](./skill-manifest-schema.md) | Unified skill + gate contract (`stages` / `verify` / Phase 2a) |
| [gates/README.md](./gates/README.md) | **Gate-as-Code** — public presets, validate CLI, migration (Phase 4.1) |
| [h4-draft-skill-security.md](./h4-draft-skill-security.md) | H4 `draft_skill` human-review checklist (Phase 4.2) |

**Phase 4.3 (T5):** Agent tools `explore_codebase` (glob→grep→read) and `edit_and_check` (edit→run_tests); T1 sequence mining via `zagens doctor --tools`. **Phase 4.4:** `zagens trace benchmark` for golden replay corpus + baseline diff.

**Fixtures** (TOML manifests, office-demo data, strict-task seed, kernel replay goldens): [`fixtures/harness/`](../../fixtures/harness/) — see [`kernel-v3-replay/README.md`](../../fixtures/harness/kernel-v3-replay/README.md) for turn-engine CI fixtures.

**Eval infrastructure, test suites, and DEMO run notebooks** are maintainer-only → `doc_Private/docs/harness/`.
