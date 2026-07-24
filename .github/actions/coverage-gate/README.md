# GitHub Actions · Zagens coverage-gate

Composite action that builds (or reuses) the `zagens` CLI and runs **`zagens coverage-gate`** against your Cargo workspace.

## What it checks

| Check | When |
|-------|------|
| `cargo fmt --check` | Always |
| `cargo clippy -D warnings` | Always |
| `cargo test --no-run` | Always |
| `cargo test` | `run-tests: true` |
| `.zagens/todo.json` completeness | `require-checklist-complete: true` |
| CRAFT `terminal_verdict` | When `.zagens/craft-ab-metrics.jsonl` exists or `task-id` set |

This is the **Layer-2 hard gate** from [COMPOSABLE_HARNESS.md](../../docs/harness/COMPOSABLE_HARNESS.md) — deterministic toolchain checks, not an LLM “LGTM”.

## Usage in this monorepo

```yaml
jobs:
  gate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: ./.github/actions/coverage-gate
        with:
          workspace: .
          require-checklist-complete: 'false'
```

## Usage in another repository

Pin a [Zagens release tag](https://github.com/didclawapp-ai/zagens/releases):

```yaml
jobs:
  gate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: didclawapp-ai/zagens/.github/actions/coverage-gate@v0.8.9
        with:
          workspace: .
          zagens-ref: v0.8.9
```

## Inputs

| Input | Default | Description |
|-------|---------|-------------|
| `workspace` | `.` | Cargo project root |
| `run-tests` | `false` | Also run `cargo test` |
| `require-checklist-complete` | `false` | LHT checklist gate |
| `task-id` | *(empty)* | CRAFT task id for verdict lookup |
| `zagens-ref` | `v0.8.9` | Tag/branch when fetching Zagens sources |
| `zagens-path` | *(empty)* | Pre-built `zagens` binary (skip compile) |
| `upload-report` | `true` | Upload JSON report artifact |
| `toolchain` | `1.96.0` | Rust toolchain for build + gate |

## Outputs

| Output | Description |
|--------|-------------|
| `passed` | `true` / `false` |
| `report-file` | Path to JSON gate report on the runner |

## Optional: headless agent review (BYOK)

The coverage-gate action does **not** call DeepSeek. For LLM-assisted PR review, run the runtime sidecar separately and keep **`[verify:]`** checklist items as the merge gate — see [GITHUB_ACTION.md](../../docs/desktop/GITHUB_ACTION.md).

## Secrets

No secrets required for coverage-gate. Optional `DEEPSEEK_API_KEY` only applies to separate headless agent / stress workflows.
