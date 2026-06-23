#!/usr/bin/env bash
# Full local gate before push: lint (CI mirror) + workspace tests + lockfile drift.
set -euo pipefail

cd "$(dirname "$0")/../.."

bash scripts/ci/verify-lint.sh

echo "==> cargo test --workspace --all-features --locked"
cargo test --workspace --all-features --locked

echo "==> multi-session parallel streaming verification"
bash scripts/ci/test-multi-session.sh

echo "==> Cargo.lock drift guard"
if ! git diff --exit-code -- Cargo.lock; then
  echo "::error::Cargo.lock changed during verify. Commit the lockfile." >&2
  exit 1
fi

echo "verify-workspace: OK"
