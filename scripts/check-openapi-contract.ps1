# Verify checked-in OpenAPI JSON + generated web-ui TS match export (D8 / D16 E5).
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

Write-Host "==> export-runtime-openapi"
cargo run -p deepseek-runtime-server --locked --bin export-runtime-openapi
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> generate:api-types"
Push-Location crates/desktop/web-ui
npm ci --include=dev
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
npm run generate:api-types
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Pop-Location

Write-Host "==> git diff (must be empty)"
git diff --exit-code -- `
  docs/tech/openapi/zagens-runtime-v1.openapi.json `
  crates/desktop/web-ui/src/api/generated/runtime-api.ts
if ($LASTEXITCODE -ne 0) {
  Write-Error "OpenAPI contract drift — re-run scripts/export-runtime-openapi.ps1 and npm run generate:api-types"
}

Write-Host "OK: OpenAPI contract matches checked-in artifacts"
