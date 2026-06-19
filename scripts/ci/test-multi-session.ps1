# Multi-session parallel streaming verification (logic selfchecks + Rust integration).
$ErrorActionPreference = 'Stop'
$Root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
Set-Location $Root

Write-Host '==> cargo test runtime_proxy (desktop SSE composite key)'
& bash scripts/ci/cargo-retry.sh test -p zagens-desktop runtime_proxy --locked
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host '==> cargo test parallel_sse_live_streams_filter_by_thread_id'
& bash scripts/ci/cargo-retry.sh test -p zagens-cli parallel_sse_live_streams_filter_by_thread_id --locked
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host '==> npm run test:multi-session (web-ui pure helpers)'
Push-Location crates/desktop/web-ui
try {
  npm run test:multi-session
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
  Pop-Location
}

Write-Host 'multi-session verification: ok'
