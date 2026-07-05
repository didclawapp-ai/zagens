#!/usr/bin/env bash
# Phase 1a Linux queue smoke — predicate gate + rollback (no bash-only oracle path).
#
# Usage (from repo root):
#   bash scripts/harness/queue-smoke.sh
#
# Requires: built `target/debug/zagens`, writable fixtures/harness/queue-smoke-linux

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

ZAGENS="${ZAGENS_BIN:-./target/debug/zagens}"
WORKSPACE="${QUEUE_SMOKE_WORKSPACE:-fixtures/harness/queue-smoke-linux}"

if [ ! -x "$ZAGENS" ]; then
  echo "Building zagens-cli debug binary..."
  cargo build -p zagens-cli --bin zagens
fi

mkdir -p "$WORKSPACE"
rm -f "$WORKSPACE"/smoke_ok.txt "$WORKSPACE"/must_not_exist.txt 2>/dev/null || true

echo "==> queue add (pass)"
"$ZAGENS" --workspace "$WORKSPACE" queue add \
  "Create smoke_ok.txt with line: ok" \
  --gate file_exists:path=smoke_ok.txt \
  --no-worktree

echo "==> queue run (expect pass)"
"$ZAGENS" --workspace "$WORKSPACE" queue run --no-worktree

echo "==> queue briefing"
"$ZAGENS" --workspace "$WORKSPACE" queue briefing

if [ ! -f "$WORKSPACE/smoke_ok.txt" ]; then
  echo "FAIL: smoke_ok.txt missing after pass run" >&2
  exit 1
fi

echo "==> queue add (intentional fail + rollback)"
"$ZAGENS" --workspace "$WORKSPACE" queue add \
  "Do nothing" \
  --gate file_exists:path=must_not_exist.txt \
  --no-worktree

echo "==> queue run (expect rollback)"
"$ZAGENS" --workspace "$WORKSPACE" queue run --no-worktree

if [ -f "$WORKSPACE/must_not_exist.txt" ]; then
  echo "FAIL: must_not_exist.txt should not exist after rollback" >&2
  exit 1
fi

EVENTS="$WORKSPACE/.zagens/queue_events.jsonl"
if [ ! -f "$EVENTS" ]; then
  echo "FAIL: queue_events.jsonl missing" >&2
  exit 1
fi

if ! grep -q 'queue_gate_result' "$EVENTS"; then
  echo "FAIL: queue_gate_result not in $EVENTS" >&2
  exit 1
fi

if ! grep -q 'queue_rollback' "$EVENTS"; then
  echo "FAIL: queue_rollback not in $EVENTS" >&2
  exit 1
fi

echo "PASS: Linux queue smoke (pass + fail rollback + events)"
