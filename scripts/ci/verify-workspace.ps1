# Full local gate before push: lint + workspace tests + lockfile drift.
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..\..")

& (Join-Path $PSScriptRoot "verify-lint.ps1")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> cargo test --workspace --all-features --locked"
cargo test --workspace --all-features --locked
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> multi-session parallel streaming verification"
& (Join-Path $PSScriptRoot "test-multi-session.ps1")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> Cargo.lock drift guard"
git diff --exit-code -- Cargo.lock
if ($LASTEXITCODE -ne 0) {
    Write-Error "Cargo.lock changed during verify. Commit the lockfile."
}

Write-Host "verify-workspace: OK"
