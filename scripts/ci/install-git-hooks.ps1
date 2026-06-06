# Install pre-commit (fmt) and pre-push (lint) hooks into .git/hooks/.
$ErrorActionPreference = "Stop"
$repoRoot = git rev-parse --show-toplevel
$hooksSrc = Join-Path $repoRoot "scripts\ci\hooks"
$hooksDst = Join-Path $repoRoot ".git\hooks"

foreach ($name in @("pre-commit", "pre-push")) {
    $src = Join-Path $hooksSrc $name
    $dst = Join-Path $hooksDst $name
    Copy-Item -Force $src $dst
    Write-Host "installed $dst"
}

Write-Host "Git hooks ready. pre-push runs scripts/ci/verify-lint.sh via Git Bash."
Write-Host "On Windows, ensure Git Bash is available, or run: pwsh scripts/ci/verify-lint.ps1 before push."
