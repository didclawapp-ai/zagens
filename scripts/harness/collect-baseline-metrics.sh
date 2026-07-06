#!/usr/bin/env bash
# Phase 0.4 — collect H2 baseline metrics (maintainer-private archive).
# Usage: bash scripts/harness/collect-baseline-metrics.sh [OUTPUT.json]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="${1:-$ROOT/doc_Private/docs/metrics/baseline-2026-H2.json}"
mkdir -p "$(dirname "$OUT")"

REPLAY_FIXTURES="$ROOT/fixtures/harness/kernel-v3-replay"
GATE_FIXTURE=""
for candidate in \
  "$ROOT/docs/harness/fixtures/microstack-completion-gate.toml" \
  "$ROOT/fixtures/harness/microstack-completion-gate.toml"; do
  if [[ -f "$candidate" ]]; then
    GATE_FIXTURE="$candidate"
    break
  fi
done
if [[ -z "$GATE_FIXTURE" ]]; then
  GATE_FIXTURE="$ROOT/docs/harness/fixtures/microstack-completion-gate.toml"
fi
GENERATED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

REPLAY_COUNT=0
if [[ -d "$REPLAY_FIXTURES" ]]; then
  REPLAY_COUNT="$(find "$REPLAY_FIXTURES" -maxdepth 1 -name '*.json' | wc -l | tr -d ' ')"
fi

TOOLS_JSON='null'
SEQ_JSON='null'
if command -v zagens >/dev/null 2>&1; then
  TOOLS_JSON="$(zagens doctor --tools --json 2>/dev/null || echo 'null')"
elif [[ -x "$ROOT/target/debug/zagens" ]]; then
  TOOLS_JSON="$("$ROOT/target/debug/zagens" doctor --tools --json 2>/dev/null || echo 'null')"
fi
if command -v jq >/dev/null 2>&1 && [[ "$TOOLS_JSON" != "null" ]]; then
  SEQ_JSON="$(printf '%s' "$TOOLS_JSON" | jq -c '.tool_sequences // null')"
fi

cat >"$OUT" <<EOF
{
  "schema": "zagens-h2-baseline-v0",
  "generated_at": "$GENERATED_AT",
  "phase": "0.4",
  "golden_replay": {
    "path": "fixtures/harness/kernel-v3-replay/",
    "fixture_json_count": $REPLAY_COUNT,
    "note": "Run kernel replay CI separately; this snapshot counts fixture files only."
  },
  "harness_fixtures": {
    "microstack_completion_gate": "${GATE_FIXTURE#$ROOT/}",
    "present": $( [[ -f "$GATE_FIXTURE" ]] && echo true || echo false )
  },
  "historical_sessions": {
    "session_ids": [],
    "note": "Maintainer: append redacted session IDs privately before formal Phase gate."
  },
  "process_metrics": {
    "avg_rework_rounds_per_task": null,
    "verify_self_heal_rate": null,
    "tool_misuse_rate": null,
    "stage_gate_false_positive_rate": null,
    "first_turn_tool_schema_tokens": null,
    "note": "Populate after T1 aggregation matures; tool telemetry seeds from tools section."
  },
  "tools_telemetry": $TOOLS_JSON,
  "tool_sequences": $SEQ_JSON
}
EOF

echo "Wrote baseline snapshot: $OUT"
