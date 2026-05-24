# B2.5 — Topic memory evaluation (metrics.json)
#
# Usage:
#   .\scripts\topic-memory-eval.ps1
#   .\scripts\topic-memory-eval.ps1 -BaselinePath .\baseline-metrics.json -Gate
#   .\scripts\topic-memory-eval.ps1 -WriteBaseline .\baseline-metrics.json

param(
    [string]$GraphPath = "",
    [string]$BaselinePath = "",
    [string]$WriteBaseline = "",
    [double]$MaxRegression = 0.05,
    [switch]$Gate,
    [switch]$Json
)

$ErrorActionPreference = "Stop"

function Resolve-MetricsPath {
    param([string]$Graph)
    if ($Graph -ne "") {
        $dir = Split-Path -Parent $Graph
        if ($dir -eq "" -or $null -eq $dir) { $dir = "." }
        return Join-Path $dir "metrics.json"
    }
    $homeDir = if ($env:USERPROFILE) { $env:USERPROFILE } else { $env:HOME }
    return Join-Path $homeDir ".deepseek\topic-memory\metrics.json"
}

function Get-EvalReport {
    param([string]$Path)
    $raw = Get-Content -Raw -Path $Path | ConvertFrom-Json
    $turns = [math]::Max(1, [double]$raw.turn_updates)
    [ordered]@{
        metrics_path = $Path
        turn_updates = [int64]$raw.turn_updates
        inject_count = [int64]$raw.inject_count
        clarification_rounds = [int64]$raw.clarification_rounds
        repeat_topic_turns = [int64]$raw.repeat_topic_turns
        clarification_rate = [double]$raw.clarification_rounds / $turns
        repeat_topic_rate = [double]$raw.repeat_topic_turns / $turns
        injects_per_10_turns = ([double]$raw.inject_count / $turns) * 10.0
        last_inject_at = $raw.last_inject_at
    }
}

$metricsPath = Resolve-MetricsPath -Graph $GraphPath
if (-not (Test-Path $metricsPath)) {
    Write-Error "metrics not found: $metricsPath (enable topic_memory and complete at least one turn)"
}

$current = Get-EvalReport -Path $metricsPath

if ($WriteBaseline -ne "") {
    Copy-Item -Force $metricsPath $WriteBaseline
    Write-Host "Wrote baseline: $WriteBaseline"
}

$report = [ordered]@{ current = $current }
$exitCode = 0

if ($BaselinePath -ne "") {
    if (-not (Test-Path $BaselinePath)) {
        Write-Error "baseline not found: $BaselinePath"
    }
    $baseline = Get-EvalReport -Path $BaselinePath
    $clDelta = $current.clarification_rate - $baseline.clarification_rate
    $rpDelta = $current.repeat_topic_rate - $baseline.repeat_topic_rate
    $regression = $clDelta -gt $MaxRegression
    $report.baseline = $baseline
    $report.clarification_rate_delta = $clDelta
    $report.repeat_topic_rate_delta = $rpDelta
    $report.regression = $regression
    if ($Gate -and $regression) {
        Write-Host "GATE FAIL: clarification_rate worsened by $clDelta (max $MaxRegression)" -ForegroundColor Red
        $exitCode = 1
    } elseif ($Gate) {
        Write-Host "GATE PASS: clarification_rate delta $clDelta" -ForegroundColor Green
    }
}

if ($Json) {
    $report | ConvertTo-Json -Depth 5
} else {
    $report | Format-List
}

exit $exitCode
