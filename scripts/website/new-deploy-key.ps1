# Generate an ed25519 key pair for GitHub Actions -> VPS rsync deploy.
# Secrets live in the zagens_website repo (not the product repo).
# Run: pwsh -File scripts/website/new-deploy-key.ps1

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$outDir = Join-Path $repoRoot 'secrets-local'
$keyPath = Join-Path $outDir 'zagens-website-deploy'

New-Item -ItemType Directory -Force -Path $outDir | Out-Null

if (Test-Path $keyPath) {
  Write-Host "Removing existing key (regenerating with no passphrase)..."
  Remove-Item -Force $keyPath, "$keyPath.pub"
}
# Empty -N in PowerShell: do not use '""' (that sets a literal passphrase).
& ssh-keygen -t ed25519 -f $keyPath -q -N ([string]::Empty) -C 'github-actions-website'

Write-Host ""
Write-Host "=== PUBLIC KEY (paste into server authorized_keys) ===" -ForegroundColor Cyan
Get-Content "$keyPath.pub"
Write-Host ""
Write-Host "=== PRIVATE KEY path (paste FULL file into GitHub Secret WEBSITE_DEPLOY_KEY) ===" -ForegroundColor Yellow
Write-Host $keyPath
Write-Host ""
Write-Host "Server one-liner (after SSH as ubuntu):" -ForegroundColor Green
$pub = (Get-Content "$keyPath.pub" -Raw).Trim()
Write-Host "echo '$pub' | sudo tee -a /home/zagens-deploy/.ssh/authorized_keys && sudo chown zagens-deploy:zagens-deploy /home/zagens-deploy/.ssh/authorized_keys && sudo chmod 600 /home/zagens-deploy/.ssh/authorized_keys"
Write-Host ""
Write-Host "Test:" -ForegroundColor Green
Write-Host "ssh -i `"$keyPath`" zagens-deploy@43.160.233.39 `"echo ok`""
