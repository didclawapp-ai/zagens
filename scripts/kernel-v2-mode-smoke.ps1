# Kernel v2 mode matrix smoke — one fast scenario per policy/scheduler combination.
#
# Uses isolated corpus configs (does not edit ~/.zagens/config.toml).
# Requires API access (keyring in merged base config or DEEPSEEK_API_KEY).
#
# Usage:
#   .\scripts\kernel-v2-mode-smoke.ps1
#   .\scripts\kernel-v2-mode-smoke.ps1 -TaskId shell-git-status
#   .\scripts\kernel-v2-mode-smoke.ps1 -Modes legacy-legacy,policy-shadow -SkipBuild

param(
    [string]$TaskId = "read-three-files",
    [string]$Modes = "legacy-legacy,policy-shadow,policy-engine,sched-shadow,sched-dag,full-v2",
    [string]$OutDir = "results/kernel-v2-corpus",
    [string]$RunLabel = "",
    [int]$TurnTimeoutSec = 0,
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$corpusScript = Join-Path $scriptDir "kernel-v2-corpus-run.ps1"

$modeMap = @{
    "legacy-legacy"   = @{ Policy = ""; Scheduler = ""; Label = "mode-legacy-legacy" }
    "policy-shadow"   = @{ Policy = "shadow"; Scheduler = ""; Label = "mode-policy-shadow" }
    "policy-engine"   = @{ Policy = "engine"; Scheduler = ""; Label = "mode-policy-engine" }
    "sched-shadow"    = @{ Policy = ""; Scheduler = "shadow"; Label = "mode-sched-shadow" }
    "sched-dag"       = @{ Policy = ""; Scheduler = "dag"; Label = "mode-sched-dag" }
    "full-v2"         = @{ Policy = "engine"; Scheduler = "dag"; Label = "mode-full-v2" }
}

$selected = @(
    $Modes.Split([char[]]@(',', ' '), [System.StringSplitOptions]::RemoveEmptyEntries) |
        ForEach-Object { $_.Trim() } |
        Where-Object { $_ }
)
if ($selected.Count -eq 0) { throw "No modes selected" }

$stamp = if ($RunLabel) { $RunLabel } else { (Get-Date).ToUniversalTime().ToString("yyyyMMdd-HHmmss") }
$parentLabel = "mode-smoke-$stamp"

Write-Host "=== Kernel v2 mode smoke ===" -ForegroundColor Cyan
Write-Host "  task:   $TaskId"
Write-Host "  modes:  $($selected -join ', ')"
Write-Host "  parent: $parentLabel"

$results = @()

foreach ($modeKey in $selected) {
    if (-not $modeMap.ContainsKey($modeKey)) {
        throw "Unknown mode '$modeKey'. Known: $($modeMap.Keys -join ', ')"
    }
    $m = $modeMap[$modeKey]
    $childLabel = $m.Label
    Write-Host ""
    Write-Host ">> $modeKey (policy=$($m.Policy ? $m.Policy : 'legacy'), scheduler=$($m.Scheduler ? $m.Scheduler : 'legacy'))" -ForegroundColor Yellow

    $args = @(
        "-File", $corpusScript,
        "-TaskId", $TaskId,
        "-RunLabel", $childLabel,
        "-OutDir", (Join-Path $OutDir $parentLabel)
    )
    if ($m.Policy) { $args += @("-ToolsPolicy", $m.Policy) }
    if ($m.Scheduler) { $args += @("-ToolsScheduler", $m.Scheduler) }
    if ($TurnTimeoutSec -gt 0) { $args += @("-TurnTimeoutSec", $TurnTimeoutSec) }
    if ($SkipBuild) { $args += "-SkipBuild" }

    & powershell @args
    if ($LASTEXITCODE -ne 0) {
        $results += [ordered]@{
            mode = $modeKey
            ok = $false
            error = "corpus exit $LASTEXITCODE"
        }
        continue
    }

    $repoRoot = Split-Path -Parent $scriptDir
    $runDir = Join-Path (Join-Path (Join-Path $repoRoot $OutDir) $parentLabel) $m.Label
    $jsonl = Join-Path $runDir "runs.jsonl"
    $row = $null
    if (Test-Path $jsonl) {
        $line = Get-Content -Path $jsonl -Tail 1
        if ($line) { $row = $line | ConvertFrom-Json }
    }
    $ok = $false
    $turnStatus = $null
    $policyShadow = $null
    $schedulerShadow = $null
    if ($row) {
        $turnStatus = $row.turn_status
        $policyShadow = $row.policy_shadow
        $schedulerShadow = $row.scheduler_shadow
        $ok = (-not $row.infra) -and ($turnStatus -eq "completed")
    }
    $results += [ordered]@{
        mode = $modeKey
        ok = $ok
        turn_status = $turnStatus
        duration_sec = if ($row) { $row.duration_sec } else { $null }
        policy_shadow = $policyShadow
        scheduler_shadow = $schedulerShadow
        run_dir = $runDir
    }
}

Write-Host ""
Write-Host "=== Summary ===" -ForegroundColor Cyan
foreach ($r in $results) {
    $mark = if ($r.ok) { "PASS" } else { "FAIL" }
    $color = if ($r.ok) { "Green" } else { "Red" }
    $extra = ""
    if ($r.policy_shadow) {
        $extra += " policy_diffs=$($r.policy_shadow.diffs)/$($r.policy_shadow.comparisons)"
    }
    if ($r.scheduler_shadow) {
        $extra += " sched_diffs=$($r.scheduler_shadow.diffs)/$($r.scheduler_shadow.comparisons)"
    }
    Write-Host ("  [{0}] {1} turn={2} dur={3}s{4}" -f $mark, $r.mode, $r.turn_status, $r.duration_sec, $extra) -ForegroundColor $color
}

$fail = @($results | Where-Object { -not $_.ok })
if ($fail.Count -gt 0) {
    Write-Host "  $($fail.Count) mode(s) failed — see run dirs under results/$OutDir/$parentLabel" -ForegroundColor Red
    exit 1
}
Write-Host "  All modes passed." -ForegroundColor Green
