# Harness documentation

Public specs and fixtures for Zagens long-horizon code evaluation and composable harness design.

## Core specs

| Doc | Description |
|-----|-------------|
| [LONG_HORIZON_CODE_TASKS.md](./LONG_HORIZON_CODE_TASKS.md) | Long-horizon code task graph, cycles, verification gates |
| [COMPOSABLE_HARNESS.md](./COMPOSABLE_HARNESS.md) | Layered completion gates (model + operator + toolchain) |
| [LHT_TEST_SUITE.md](./LHT_TEST_SUITE.md) | Regression test suite layout |
| [LHT_EVAL_INFRASTRUCTURE.md](./LHT_EVAL_INFRASTRUCTURE.md) | L2 evaluation infrastructure |

## Fixtures & test cases

| Path | Description |
|------|-------------|
| [fixtures/](./fixtures/) | TOML/JSON harness configs, office-demo data, strict-task seed |
| [test-cases/](./test-cases/) | Written scenarios (DEMO3–8, microstack, redis, SWE-bench sample) |

## Maintainer-only material

Internal integration proposals, paper drafts, and session handoffs were moved to `doc_Private/docs/harness/` and are not published in this repository.
