# Zagens Kernel Trace Report (KTR)

Offline Flight Recorder UI for Kernel V3 `KernelEvent` logs.

## Dev

```bash
cd tools/trace-report
npm install
npm run dev
npm run build        # → dist/report.html (single-file shell)
```

## Export

```bash
cargo build -p zagens-cli --bin zagens

# P0: golden fixture
zagens trace export \
  --fixture fixtures/harness/kernel-v3-replay/lht_continue.json \
  --out /tmp/lht_continue.html

# P1: live thread (sessions.db + runtime store)
zagens trace export \
  --thread thr_abc123 \
  --out /tmp/thr_abc123.html

# JSON bundle only
zagens trace export --fixture fixtures/.../lht_continue.json --format bundle --out trace.bundle.json

# Thread without harness snapshot
zagens trace export --thread thr_abc123 --include-harness false --out report.html
```

## Views

| Tab | P0 | P1 |
|-----|----|----|
| Overview | KPI + coherence badge | + multi-turn |
| Timeline | Model / Tools / Guards | same |
| Turn Map | — | per-turn coherence + effect totals |
| Memory | — | compaction + capacity from events |
| Harness | — | offline task graph from thread store |

## CI

```bash
bash scripts/ci/verify-trace-report.sh
```

Spec: `doc_Private/docs/tech/KERNEL_TRACE_REPORT_PLAN.md`
