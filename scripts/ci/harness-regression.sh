#!/usr/bin/env bash
# L7: headless LHT/CRAFT harness regression runner.
#
# Runs the existing harness integration tests (mock-LLM safe) and the
# cross-platform coverage-gate check.  Designed to run on any POSIX shell
# CI runner without requiring a live DeepSeek API key (the long-run stress
# test with a real API lives in harness-regression.yml and is gated on
# DEEPSEEK_API_KEY being set).
#
# Usage: bash scripts/ci/harness-regression.sh [--with-longrun]
#   --with-longrun   Also run the 35+ min R-015 baseline (needs API key).
#
# Exit codes: 0 = all checks green, 1 = at least one check failed.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

WITH_LONGRUN=false
for arg in "$@"; do
  [ "$arg" = "--with-longrun" ] && WITH_LONGRUN=true
done

PASS=0
FAIL=0

# Helper: run a command, print result, accumulate counters.
check() {
  local label="$1"; shift
  echo "==> ${label}"
  if "$@"; then
    echo "    ✓ PASS"
    PASS=$((PASS + 1))
  else
    echo "    ✗ FAIL"
    FAIL=$((FAIL + 1))
  fi
  echo
}

# ── Step 1: fast unit + lib tests ──────────────────────────────────────────
check "Lib tests (zagens-cli)" \
  bash scripts/ci/cargo-retry.sh test -p zagens-cli --lib --locked

# ── Step 2: integration contract tests ─────────────────────────────────────
check "Runtime sidecar contract" \
  cargo test -p zagens-cli --lib sidecar_contract_full_lifecycle --locked

check "Runtime sidecar binary contract" \
  cargo test -p zagens-cli --test sidecar_binary_contract --locked

check "Headless CLI binary contract" \
  cargo test -p zagens-cli --test zagens_cli_contract --locked

check "Headless exec mock (e2e with mock LLM)" \
  cargo test -p zagens-cli --lib exec_agent_json_e2e_with_mock_llm --locked

# ── Step 3: coverage-gate (Layer-2 hard gate) ──────────────────────────────
# Build release binary first so coverage-gate can call it.
echo "==> Building zagens release binary for gate check"
bash scripts/ci/cargo-retry.sh build -p zagens-cli --release --locked --bin zagens 2>&1 | tail -3
echo

check "coverage-gate (fmt + clippy + compile)" \
  ./target/release/zagens coverage-gate --no-fail --json

# ── Step 4: optional 35-min longrun stress test ────────────────────────────
if "$WITH_LONGRUN"; then
  if [ -z "${DEEPSEEK_API_KEY:-}" ]; then
    echo "==> Skipping longrun stress test (DEEPSEEK_API_KEY not set)"
  else
    check "R-015 longrun baseline (3 × 50 turns)" \
      pwsh -File scripts/runtime-longrun-baseline.ps1 -Runs 3 -Gate -Model deepseek-v4-pro
  fi
fi

# ── Summary ────────────────────────────────────────────────────────────────
echo "═══════════════════════════════════════"
echo "  Harness regression: ${PASS} passed, ${FAIL} failed"
echo "═══════════════════════════════════════"
[ "$FAIL" -eq 0 ]
