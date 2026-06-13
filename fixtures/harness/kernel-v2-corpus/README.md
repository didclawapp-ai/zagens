# Kernel v2 golden corpus (M0.1)

Fixed scenario set for kernel-v2 baseline measurement and bucketed acceptance
benchmarks. See `doc_Private/docs/tech/AGENT_KERNEL_V2_TIER1_PLAN.md` (M0.1)
and the proposal §13 metric buckets.

| Path | Purpose |
|------|---------|
| [scenarios.toml](./scenarios.toml) | Scenario definitions (`[[task]]`, lht-harness compatible) |
| [workspace-seed/](./workspace-seed/) | Small fixed Rust codebase ("minilang") scenarios operate on |

## Batch shapes

| `batch_shape` | Scenarios | Measures |
|---------------|-----------|----------|
| `pure_read` | `read-three-files`, `grep-glob-survey`, `read-compare-modules`, `read-long-thinking`* | read/grep/glob batches — M4 target: step latency −20% |
| `shell_degradable` | `shell-git-status`, `shell-rg-search`, `shell-list-files` | read-only shell commands (§8.1.1 dynamic footprint) — reported separately |
| `write` | `write-append-readme`, `write-new-doc`, `write-rename-fn` | edit/write batches — no-regression gate |

\* `read-long-thinking` is the long-thinking scenario (speculative execution
upside, M4.5 evaluation input).

## Replay

```powershell
.\scripts\kernel-v2-corpus-run.ps1                       # all scenarios
.\scripts\kernel-v2-corpus-run.ps1 -TaskId read-three-files
.\scripts\kernel-v2-corpus-run.ps1 -Repeat 3 -RunLabel baseline-main
.\scripts\kernel-v2-corpus-run.ps1 -RunLabel m4-dag -ToolsScheduler dag
.\scripts\kernel-v2-corpus-run.ps1 -RunLabel m3-shadow -ToolsPolicy shadow
```

Pass `-ToolsScheduler legacy|shadow|dag` to set `[tools] scheduler` in the
merged sidecar config (default: legacy). Pass `-ToolsPolicy legacy|shadow|engine`
for M3 policy shadow bake (default: legacy). `runs.jsonl` records
`tools_scheduler` and `tools_policy`.

### M4 bucketed compare (fair baseline)

Use a **fresh legacy rerun** on the same release binary as the DAG candidate
(not the older `m0.2-baseline` snapshot, which mixed infra retries and predates
report-parser fixes):

```powershell
.\scripts\kernel-v2-corpus-run.ps1 -RunLabel m4-legacy-rerun -ToolsScheduler legacy
.\scripts\kernel-v2-corpus-run.ps1 -RunLabel m4-dag -ToolsScheduler dag

python scripts/kernel_v2_corpus_compare.py `
  results/kernel-v2-corpus/m4-legacy-rerun/report.json `
  results/kernel-v2-corpus/m4-dag/report.json

# CI-style gate (pure_read step p50 must improve ≥20%):
python scripts/kernel_v2_corpus_compare.py `
  results/kernel-v2-corpus/m4-legacy-rerun/report.json `
  results/kernel-v2-corpus/m4-dag/report.json --gate
```

The compare script also prints `pure_read tool_step p50` and `tool_phase p50`
(informational; the formal M4 gate remains `step_total_ms` p50).

**M4 live result (2026-06-13):** fair legacy vs DAG pure_read step p50 **+8.7%**
(8039 → 7341 ms); formal −20% gate **not met** — see Tier-1 plan M4 notes.

### M3 policy shadow bake

```powershell
.\scripts\kernel-v2-corpus-run.ps1 -RunLabel m3-shadow -ToolsPolicy shadow
```

### Mode matrix smoke (all policy/scheduler combinations)

One fast scenario per combination (`read-three-files` by default); uses
isolated merged configs (does not edit `~/.zagens/config.toml`):

```powershell
.\scripts\kernel-v2-mode-smoke.ps1
.\scripts\kernel-v2-mode-smoke.ps1 -TaskId shell-git-status
.\scripts\kernel-v2-mode-smoke.ps1 -Modes "policy-engine,full-v2" -TaskId write-append-readme
```

Modes: `legacy-legacy`, `policy-shadow`, `policy-engine`, `sched-shadow`,
`sched-dag`, `full-v2`. Summary prints turn status and shadow diff counts.

**Tier-1 closure (2026-06-13):** M0–M5 engineering delivery + accelerated gates
signed off; defaults remain `policy=legacy`, `scheduler=legacy`. G milestone
(delete legacy, flip defaults) is follow-up. Maintainer plan:
`doc_Private/docs/tech/AGENT_KERNEL_V2_TIER1_PLAN.md`.

Each scenario run probes `GET /v1/runtime/kernel-shadow` before the sidecar stops;
counters are copied into `runs.jsonl` (`policy_shadow` / `scheduler_shadow`).

```powershell
python scripts/kernel_v2_shadow_bake_report.py results/kernel-v2-corpus/m3-shadow
python scripts/kernel_v2_shadow_bake_report.py results/kernel-v2-corpus/m3-shadow --gate
```

After each sidecar run with `-ToolsPolicy shadow`, you can also inspect shadow
counters via the built-in **`diagnostics` tool** (JSON field `policy_shadow`:
`comparisons`, `diffs`, `diff_rate_pct`). Counters are process-local per sidecar
instance. Target: **< 0.1% diff_rate for 2 weeks** before defaulting
`[tools] policy` to `engine`.

One sidecar per scenario run (fresh `DEEPSEEK_RUNTIME_DIR`, ephemeral
workspace seeded from `workspace-seed/`). Captures the persisted event
stream (`events?since_seq=0&replay_only=1`) and thread detail, then
`scripts/kernel_v2_corpus_report.py` segments each turn into steps and emits
`report.json` with per-step latency plus per-batch-shape aggregates
(mean / p50 / p95, KV cache hit rate, token totals).

Reused by: M0.2 baseline report → M4 bucketed benchmarks → M5 prefix CI
input → (if exercised) Phase 3 replay fixtures.

### M5 prefix CI (local / CI)

```powershell
python scripts/kernel_v2_prefix_ci.py
```

Runs `cargo test -p zagens-cli request_fingerprint` (static layer stability,
handoff/full separation, workspace-seed fixture). Live corpus runs emit
`turn.prefix_fingerprint` SSE events when the sidecar computes fingerprints;
`kernel_v2_corpus_report.py` summarizes them under `prefix_fingerprint_summary`.

After a live corpus replay, the runner passes
`--assert-prefix-stability --require-fingerprints` so each successful scenario
must emit fingerprint events; **static-prefix stability is enforced on
`pure_read` only** (write/shell turns may activate deferred tools mid-turn).
Use `--prefix-stability-shapes all` on the report script for the strict check.
