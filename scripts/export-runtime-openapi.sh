#!/usr/bin/env bash
# Regenerate docs/tech/openapi/zagens-runtime-v1.openapi.json (D8).
# After export, run `npm run generate:api-types` in crates/desktop/web-ui (or ./scripts/check-openapi-contract.sh).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
cargo run -p zagens-cli --bin export-runtime-openapi
echo "OK: docs/tech/openapi/zagens-runtime-v1.openapi.json"
