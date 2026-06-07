# Move non-design docs from docs/ to doc_Private/; relocate harness fixtures to fixtures/harness/.
$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
Set-Location $root

function Move-ToPrivate {
    param([string]$Rel)
    $src = Join-Path $root $Rel
    if (-not (Test-Path -LiteralPath $src)) {
        Write-Warning "skip missing: $Rel"
        return
    }
    $dst = Join-Path $root "doc_Private/$Rel"
    $parent = Split-Path $dst -Parent
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    if (Test-Path -LiteralPath $dst) { Remove-Item -LiteralPath $dst -Recurse -Force }
    Move-Item -LiteralPath $src -Destination $dst -Force
    git rm -r --cached --ignore-unmatch -- $Rel 2>$null | Out-Null
    Write-Host "private: $Rel"
}

# 1) Harness fixtures → fixtures/harness/
$fixturesSrc = Join-Path $root "docs/harness/fixtures"
$fixturesDst = Join-Path $root "fixtures/harness"
if (Test-Path -LiteralPath $fixturesSrc) {
    New-Item -ItemType Directory -Force -Path (Split-Path $fixturesDst -Parent) | Out-Null
    if (Test-Path -LiteralPath $fixturesDst) { Remove-Item -LiteralPath $fixturesDst -Recurse -Force }
    git mv docs/harness/fixtures fixtures/harness
    Write-Host "moved: docs/harness/fixtures -> fixtures/harness"
}

# 2) Dev guide → repo root
$localDev = Join-Path $root "docs/LOCAL_DEV_VERIFY.md"
if (Test-Path -LiteralPath $localDev) {
    if (Test-Path -LiteralPath (Join-Path $root "LOCAL_DEV_VERIFY.md")) {
        Remove-Item -LiteralPath (Join-Path $root "LOCAL_DEV_VERIFY.md") -Force
    }
    git mv docs/LOCAL_DEV_VERIFY.md LOCAL_DEV_VERIFY.md
    Write-Host "moved: docs/LOCAL_DEV_VERIFY.md -> LOCAL_DEV_VERIFY.md"
}

# 3) Non-spec docs → doc_Private
$privateRel = @(
    "docs/REPO_SPLIT.md",
    "docs/user/README.md",
    "docs/desktop/VERSIONING.md",
    "docs/desktop/UPDATER.md",
    "docs/desktop/SMARTSCREEN.md",
    "docs/desktop/I18N_PLAN.md",
    "docs/harness/LHT_TEST_SUITE.md",
    "docs/harness/LHT_EVAL_INFRASTRUCTURE.md",
    "docs/tech/adr/G2_GATE_ACCEPTANCE.md",
    "docs/tech/adr/A2_A3_SIGNOFF.md"
)

foreach ($rel in $privateRel) { Move-ToPrivate $rel }

$privateDirs = @(
    "docs/harness/test-cases",
    "docs/skill"
)

foreach ($rel in $privateDirs) { Move-ToPrivate $rel }

Write-Host "done. Public docs/ is design SPEC only; fixtures under fixtures/harness/"
