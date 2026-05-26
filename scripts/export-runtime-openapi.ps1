# Regenerate docs/tech/openapi/zagens-runtime-v1.openapi.json (D8).
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
cargo run -p deepseek-runtime-server --bin export-runtime-openapi
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Host "OK: docs/tech/openapi/zagens-runtime-v1.openapi.json"
