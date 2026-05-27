#!/usr/bin/env bash
# Verify checked-in OpenAPI JSON + generated web-ui TS match export (D8 / D16 E5).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "==> export-runtime-openapi"
cargo run -p deepseek-runtime-server --locked --bin export-runtime-openapi

echo "==> generate:api-types"
cd crates/desktop/web-ui
npm ci --include=dev
npm run generate:api-types
cd "$ROOT"

echo "==> git diff (must be empty)"
git diff --exit-code -- \
  docs/tech/openapi/zagens-runtime-v1.openapi.json \
  crates/desktop/web-ui/src/api/generated/runtime-api.ts

echo "OK: OpenAPI contract matches checked-in artifacts"
