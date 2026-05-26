#!/usr/bin/env bash
# Regenerate docs/tech/openapi/zagens-runtime-v1.openapi.json (D8).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
cargo run -p deepseek-tui --no-default-features --features openapi-export,json,toml \
  --bin export-runtime-openapi
echo "OK: docs/tech/openapi/zagens-runtime-v1.openapi.json"
