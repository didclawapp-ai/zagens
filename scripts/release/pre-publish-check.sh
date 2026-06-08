#!/usr/bin/env bash
# Pre-crates.io checklist: version sync, fmt, tests, and leaf-crate publish dry-runs.
#
# Usage:
#   bash scripts/release/pre-publish-check.sh
#   bash scripts/release/pre-publish-check.sh --skip-tests   # faster; versions + fmt + leaf dry-run only
#
# Mirrors CI gates relevant to the zagens-cli publish chain. Run on a clean commit before
# `scripts/release/publish-crates.sh --publish`.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

skip_tests=0
for arg in "$@"; do
  case "$arg" in
    --skip-tests) skip_tests=1 ;;
    -h | --help)
      echo "Usage: bash scripts/release/pre-publish-check.sh [--skip-tests]"
      exit 0
      ;;
    *)
      echo "Unknown argument: $arg" >&2
      exit 2
      ;;
  esac
done

if [[ -n "$(git status --porcelain 2>/dev/null || true)" ]]; then
  echo "WARNING: working tree is not clean — publish will refuse without --allow-dirty." >&2
fi

echo "==> Version drift"
bash scripts/release/check-versions.sh

echo "==> Formatting"
cargo fmt --all -- --check

if [[ "${skip_tests}" -eq 0 ]]; then
  echo "==> Workspace tests (excluding zagens-cli integration surface)"
  bash scripts/ci/cargo-retry.sh test --workspace --all-features --locked --exclude zagens-cli

  echo "==> zagens-cli lib tests"
  bash scripts/ci/cargo-retry.sh test -p zagens-cli --lib --all-features --locked

  if [[ "$(uname -s 2>/dev/null || echo unknown)" == "Linux" ]]; then
    echo "==> Sidecar + CLI contract tests (Linux only)"
    cargo test -p zagens-cli --lib sidecar_contract_full_lifecycle --locked
    cargo test -p zagens-cli --test sidecar_binary_contract --locked
    cargo test -p zagens-cli --test zagens_cli_contract --locked
    cargo test -p zagens-cli --lib exec_agent_json_e2e_with_mock_llm --locked
  else
    echo "==> Skipping Linux-only contract tests on $(uname -s 2>/dev/null || echo this host)"
    cargo test -p zagens-cli --test zagens_cli_contract --locked
  fi

  echo "==> Release CLI binary smoke"
  bash scripts/ci/cargo-retry.sh build -p zagens-cli --release --locked --bin zagens --bin zagens-runtime
else
  echo "==> Skipping test suite (--skip-tests)"
fi

echo "==> Leaf crate publish dry-runs (no internal path deps on other zagens-* crates)"
leaf_crates=(zagens-protocol zagens-secrets zagens-topic-memory)
for pkg in "${leaf_crates[@]}"; do
  echo "    dry-run ${pkg}"
  cargo publish -p "${pkg}" --dry-run --allow-dirty
done

echo ""
echo "OK: pre-publish checks passed."
echo "Next: bash scripts/release/publish-crates.sh --dry-run   # after deps hit crates.io, full chain"
echo "      bash scripts/release/publish-crates.sh --publish     # requires cargo login + clean tree"
