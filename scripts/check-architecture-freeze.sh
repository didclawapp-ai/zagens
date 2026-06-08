#!/usr/bin/env bash
# D17 optional — run architecture freeze checks locally (no new gate rules).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "==> architecture_invariants (runtime-server)"
cargo test -p zagens-cli --test architecture_invariants --locked

echo "==> architecture_boundary (desktop)"
cargo test -p zagens-desktop --test architecture_boundary --locked

echo "==> OpenAPI contract"
"$ROOT/scripts/check-openapi-contract.sh"

echo "OK: architecture freeze checks passed"
