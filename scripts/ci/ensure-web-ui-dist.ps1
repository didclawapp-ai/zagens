# Desktop clippy needs crates/desktop/web-ui/dist (Tauri frontendDist).
$ErrorActionPreference = "Stop"
$dist = Join-Path $PSScriptRoot "..\..\crates\desktop\web-ui\dist\index.html"
if (Test-Path $dist) { return }

Write-Host "==> web-ui dist missing; npm ci + build (required for desktop clippy)"
Push-Location (Join-Path $PSScriptRoot "..\..\crates\desktop\web-ui")
try {
    npm ci --include=dev
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    npm run build
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}
