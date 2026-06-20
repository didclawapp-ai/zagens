#!/usr/bin/env bash
# Smoke-test Kernel Trace Report export (P0 fixtures → HTML).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

TEMPLATE="$ROOT/tools/trace-report/dist/report.html"
OUT_DIR="$ROOT/target/trace-report-smoke"

if [[ ! -f "$TEMPLATE" ]]; then
  echo "Building trace-report frontend…"
  (cd "$ROOT/tools/trace-report" && npm ci && npm run build)
fi

if [[ ! -f "$TEMPLATE" ]]; then
  echo "Missing $TEMPLATE after build" >&2
  exit 1
fi

echo "Building zagens CLI…"
cargo build -p zagens-cli --bin zagens --quiet

ZAGENS="$ROOT/target/debug/zagens"
if [[ ! -x "$ZAGENS" ]]; then
  ZAGENS="$ROOT/target/debug/zagens.exe"
fi

mkdir -p "$OUT_DIR"

FAIL=0
FIXTURES=(
  pure_read.json
  write_batch.json
  lht_continue.json
  loop_guard.json
  scratchpad_compaction.json
  cycle_handoff.json
  overflow_recovery.json
  capacity_checkpoint.json
  manual_compaction.json
  deferred_activation.json
  memory_plane_query.json
  resume_thread_parity.json
  layered_context_seam.json
  message_body_rebuild.json
  system_prompt_refresh.json
)

for name in "${FIXTURES[@]}"; do
  fixture="$ROOT/fixtures/harness/kernel-v3-replay/$name"
  out="$OUT_DIR/${name%.json}.html"
  echo "Export $name …"
  if ! "$ZAGENS" trace export --fixture "$fixture" --out "$out"; then
    FAIL=1
    continue
  fi
  if ! grep -q 'coherence_ok' "$out"; then
    echo "Missing coherence badge in $out" >&2
    FAIL=1
  fi
  if ! grep -q 'Kernel Trace Report' "$out"; then
    echo "Missing title in $out" >&2
    FAIL=1
  fi
done

echo "Running zagens-core trace_bundle tests…"
cargo test -p zagens-core trace_ --quiet

echo "Compare fixture smoke…"
COMPARE_OUT="$OUT_DIR/lht_vs_loop_guard.compare.html"
if ! "$ZAGENS" trace compare \
  --left-fixture "$ROOT/fixtures/harness/kernel-v3-replay/lht_continue.json" \
  --right-fixture "$ROOT/fixtures/harness/kernel-v3-replay/loop_guard.json" \
  --out "$COMPARE_OUT"; then
  FAIL=1
elif ! grep -q 'event_sequence_diff' "$COMPARE_OUT"; then
  echo "Missing compare diff marker in $COMPARE_OUT" >&2
  FAIL=1
fi

if [[ "$FAIL" -ne 0 ]]; then
  echo "trace-report smoke FAILED" >&2
  exit 1
fi

echo "trace-report smoke OK ($OUT_DIR)"
