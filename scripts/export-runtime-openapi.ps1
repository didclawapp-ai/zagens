# Regenerate docs/tech/openapi/zagens-runtime-v1.openapi.json (D8).
# After export, run `npm run generate:api-types` in crates/desktop/web-ui (or ./scripts/check-openapi-contract.ps1).
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
cargo run -p zagens-cli --bin export-runtime-openapi
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Host "OK: docs/tech/openapi/zagens-runtime-v1.openapi.json"
