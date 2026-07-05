# Harness design specifications

Public **feature specs** for long-horizon code tasks and composable completion gates.

| Doc | Description |
|-----|-------------|
| [LONG_HORIZON_CODE_TASKS.md](./LONG_HORIZON_CODE_TASKS.md) | Task graph, cycles, verification gates, LHT phases |
| [COMPOSABLE_HARNESS.md](./COMPOSABLE_HARNESS.md) | Layered completion gates (operator / model / toolchain) |
| [skill-manifest-schema.md](./skill-manifest-schema.md) | Unified skill + gate contract (`stages` / `verify` / Phase 2a) |

**Fixtures** (TOML manifests, office-demo data, strict-task seed, kernel replay goldens): [`fixtures/harness/`](../../fixtures/harness/) — see [`kernel-v3-replay/README.md`](../../fixtures/harness/kernel-v3-replay/README.md) for turn-engine CI fixtures.

**Eval infrastructure, test suites, and DEMO run notebooks** are maintainer-only → `doc_Private/docs/harness/`.
