# D17 optional — run architecture freeze checks locally (no new gate rules).
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

Write-Host "==> architecture_invariants (runtime-server)"
cargo test -p zagens-cli --test architecture_invariants --locked
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> architecture_boundary (desktop)"
cargo test -p zagens-desktop --test architecture_boundary --locked
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> OpenAPI contract"
& "$PSScriptRoot/check-openapi-contract.ps1"
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "OK: architecture freeze checks passed"
