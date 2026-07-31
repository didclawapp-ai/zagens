# Harness fixtures

Executable assets for LHT evaluation — **not** prose documentation.

| Path | Purpose |
|------|---------|
| [kernel-v2-corpus/](./kernel-v2-corpus/) | Kernel v2 golden corpus: batch-shape latency scenarios (M0.1) |
| [strict-task-seed/](./strict-task-seed/) | Minimal Go seed for strict harness smoke |
| `lht-*.toml` / `*.json` | Harness task manifests and eval profiles |
| `windows-enterprise-requirements.toml` | Example enterprise overlay for Windows elevated sandbox (PR-3.6) |

Office P0 demo fixtures were removed with built-in Office mode (2026-07); use external `zagens-office` CLI + skill for document workflows.

Design specs: [`docs/harness/`](../docs/harness/README.md).
