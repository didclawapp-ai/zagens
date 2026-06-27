#!/usr/bin/env bash
# Multi-session parallel streaming verification (web-ui Vitest/ESLint + Rust integration).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

echo "==> cargo test runtime_proxy (desktop SSE composite key)"
bash scripts/ci/cargo-retry.sh test -p zagens-desktop runtime_proxy --locked

echo "==> cargo test parallel_sse_live_streams_filter_by_thread_id"
bash scripts/ci/cargo-retry.sh test -p zagens-cli parallel_sse_live_streams_filter_by_thread_id --locked

echo "==> npm run lint && npm test (web-ui Vitest suite)"
(
  cd crates/desktop/web-ui
  npm run lint
  npm test
)

echo "multi-session verification: ok"
