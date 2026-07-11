# GitHub Actions integration

Zagens ships a **composite GitHub Action** that runs the portable Layer-2 completion gate (`zagens coverage-gate`) in any Rust workspace — the same gate used by CRAFT / LHT harness locally.

**Action path:** [`.github/actions/coverage-gate`](../../.github/actions/coverage-gate)  
**Upstream repo:** [didclawapp-ai/zagens](https://github.com/didclawapp-ai/zagens)

---

## Quick start

### Same monorepo (dogfood)

```yaml
name: Completion gate
on:
  pull_request:
  workflow_dispatch:

permissions:
  contents: read

jobs:
  coverage-gate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: ./.github/actions/coverage-gate
        with:
          workspace: .
```

### External Rust project

Pin a release tag so the action can build the CLI:

```yaml
- uses: didclawapp-ai/zagens/.github/actions/coverage-gate@v0.8.6
  with:
    workspace: .
    zagens-ref: v0.8.6
    run-tests: 'true'   # optional, slower
```

Artifacts: **`zagens-coverage-gate-report`** JSON (`checks[]`, `passed`, `total_ms`).

---

## What gets verified

The gate is **deterministic** — no model inference in the default path:

1. `cargo fmt --check`
2. `cargo clippy -- -D warnings`
3. `cargo test --no-run` (compile tests)
4. Optional: full `cargo test` (`run-tests`)
5. Optional: `.zagens/todo.json` all items completed (`require-checklist-complete`)
6. Optional: CRAFT `terminal_verdict == PASS` in `.zagens/craft-ab-metrics.jsonl`

Align LHT acceptance items with **`[verify: <command>]`** in the plan so the model cannot “claim done” without toolchain proof. See [COMPOSABLE_HARNESS.md](../harness/COMPOSABLE_HARNESS.md).

---

## LHT / CRAFT projects

After a desktop or headless CRAFT run leaves artifacts under `.zagens/`:

```yaml
- uses: ./.github/actions/coverage-gate
  with:
    require-checklist-complete: 'true'
    task-id: 'my-task-123'   # optional; latest record if omitted
```

---

## Headless agent on PRs (optional, BYOK)

Coverage-gate alone does not post review comments. For agent-assisted review:

1. Add secret **`DEEPSEEK_API_KEY`** (or your provider key via runtime config).
2. Install/build **`zagens`** + **`zagens-runtime`** (see [LOCAL_DEV_VERIFY.md](../../LOCAL_DEV_VERIFY.md)).
3. Run a headless turn against the PR diff context, **but keep merge blocking on coverage-gate + `[verify:]`** — not on model prose.

Example sketch (maintainer-owned; not an official Action yet):

```yaml
- name: Headless review (non-blocking)
  env:
    DEEPSEEK_API_KEY: ${{ secrets.DEEPSEEK_API_KEY }}
  run: |
    ./target/release/zagens-runtime serve &
    # … POST /v1/stream or task API with PR prompt …
  continue-on-error: true

- uses: ./.github/actions/coverage-gate
  with:
    require-checklist-complete: 'true'
```

Scheduled stress + gate regression already runs in [`.github/workflows/harness-regression.yml`](../../.github/workflows/harness-regression.yml).

---

## GitLab CI (manual)

Equivalent local command after installing `zagens`:

```bash
zagens coverage-gate --workspace "$CI_PROJECT_DIR" --json
```

Use the same checklist / CRAFT flags as the GitHub Action inputs.

---

## Related docs

| Doc | Topic |
|-----|--------|
| [COMPOSABLE_HARNESS.md](../harness/COMPOSABLE_HARNESS.md) | Layer-2 gates, `[verify:]` |
| [LOCAL_DEV_VERIFY.md](../../LOCAL_DEV_VERIFY.md) | Local `coverage-gate` |
| [HOOKS.md](./HOOKS.md) | Lifecycle hooks (desktop) |
