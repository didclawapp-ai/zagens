# Full local gate before push: lint + workspace tests + lockfile drift.
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..\..")

& (Join-Path $PSScriptRoot "verify-lint.ps1")

Write-Host "==> cargo test --workspace --all-features --locked"
cargo test --workspace --all-features --locked
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> Cargo.lock drift guard"
git diff --exit-code -- Cargo.lock
if ($LASTEXITCODE -ne 0) {
    Write-Error "Cargo.lock changed during verify. Commit the lockfile."
}

Write-Host "verify-workspace: OK"
