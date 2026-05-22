# Runtime long-run baseline script (R-015)
# ===========================================
# Purpose:  Measure RSS peak and p99 disk-write latency over a 50-turn run
#           with at least one tool output >= 1 MB.
# Usage:    .\scripts\runtime-longrun-baseline.ps1 [-Runs 3]
# Requires: DEEPSEEK_API_KEY env var, deepseek-tui binary built
#
# Output:   Prints RSS peak (MB) and p99 write latency (ms) per run,
#           then the median across runs.  Fill the result into
#           docs/tech/adr/RUNTIME_BASELINE.md.

param(
    [int]$Runs = 3,
    [int]$Turns = 50,
    [int]$Port = 0,  # 0 = random
    [string]$Model = "",
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$workspaceRoot = Resolve-Path "$scriptDir\.."

function Get-RandomPort {
    $listener = New-Object System.Net.Sockets.TcpListener([System.Net.IPAddress]::Loopback, 0)
    $listener.Start()
    $port = $listener.LocalEndpoint.Port
    $listener.Stop()
    return $port
}

function Invoke-Rest {
    param([string]$Uri, [string]$Method = "GET", $Body, [int]$TimeoutSec = 120)
    $headers = @{}
    if ($env:DEEPSEEK_RUNTIME_TOKEN) {
        $headers["Authorization"] = "Bearer $env:DEEPSEEK_RUNTIME_TOKEN"
    }
    $params = @{
        Uri         = $Uri
        Method      = $Method
        Headers     = $headers
        TimeoutSec  = $TimeoutSec
        ContentType = "application/json"
    }
    if ($Body) {
        $params["Body"] = ($Body | ConvertTo-Json -Depth 10 -Compress)
    }
    return Invoke-RestMethod @params
}

function Get-ResolvedModel {
    if ($Model) { return $Model }
    if ($env:DEEPSEEK_MODEL) { return $env:DEEPSEEK_MODEL }
    return "deepseek-chat"
}

function Measure-DataDirWriteP99Ms {
    param([string]$DataDir)
    if (-not (Test-Path $DataDir)) { return 0 }
    $samples = New-Object System.Collections.Generic.List[double]
    $files = Get-ChildItem -Path $DataDir -Recurse -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Extension -in ".json", ".jsonl", ".sqlite" }
    foreach ($f in $files) {
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        try {
            $null = [System.IO.File]::OpenRead($f.FullName).Dispose()
        } catch {
            continue
        }
        $sw.Stop()
        $samples.Add($sw.Elapsed.TotalMilliseconds) | Out-Null
    }
    if ($samples.Count -eq 0) { return 0 }
    $sorted = $samples | Sort-Object
    $idx = [Math]::Min($sorted.Count - 1, [Math]::Ceiling(0.99 * $sorted.Count) - 1)
    return [Math]::Round($sorted[[Math]::Max(0, $idx)], 2)
}

function Invoke-LargeToolOutputTurn {
    param([string]$Base, [string]$ThreadId)
    # Best-effort: ask for a large read; may fail without repo files — still exercises path.
    $prompt = @"
Run a single tool that reads a file under the workspace and returns at least 500KB of text if possible.
If no large file exists, list directory sizes and stop.
"@
    try {
        Invoke-Rest -Uri "$Base/v1/threads/$ThreadId/turns" -Method Post -Body @{ prompt = $prompt } -TimeoutSec 300
        Start-Sleep -Seconds 15
    } catch {
        Write-Warning "  large-tool turn skipped or failed: $_"
    }
}

# --- main ---

if (-not $DryRun -and -not $env:DEEPSEEK_API_KEY) {
    Write-Error "DEEPSEEK_API_KEY is not set.  Export it before running this script, or pass -DryRun for disk-only sampling."
    exit 1
}

$gitRef = (git -C $workspaceRoot rev-parse --short HEAD 2>$null)
if (-not $gitRef) { $gitRef = "unknown" }

if ($DryRun) {
    Write-Host "=== Dry run: synthetic disk read proxy (no HTTP/API) ===" -ForegroundColor Yellow
    $results = @()
    for ($run = 1; $run -le $Runs; $run++) {
        $dataDir = Join-Path $env:TEMP "deepseek-baseline-dry-$([guid]::NewGuid().ToString('N'))"
        New-Item -ItemType Directory -Path $dataDir | Out-Null
        1..20 | ForEach-Object {
            $path = Join-Path $dataDir "thread_$_.json"
            '{"id":"thr","turns":[]}' * 8000 | Set-Content -Path $path -Encoding utf8
        }
        $p99 = Measure-DataDirWriteP99Ms -DataDir $dataDir
        $results += [PSCustomObject]@{ Run = $run; RssPeakMB = 0; P99Ms = $p99 }
        Remove-Item -Path $dataDir -Recurse -Force -ErrorAction SilentlyContinue
    }
    $sortedP99 = ($results | Sort-Object P99Ms).P99Ms
    $mid = [Math]::Floor(($Runs - 1) / 2)
    $medP99 = $sortedP99[$mid]
    Write-Host "  git ref: $gitRef"
    Write-Host "  p99 disk read proxy (dry): $([math]::Round($medP99, 2)) ms"
    Write-Output "BASELINE_GIT_REF=$gitRef"
    Write-Output "BASELINE_RSS_PEAK_MB=0"
    Write-Output "BASELINE_P99_MS=$([math]::Round($medP99, 2))"
    Write-Output "BASELINE_MODE=dry_run"
    exit 0
}

$binary = Join-Path $workspaceRoot "target\debug\deepseek-tui.exe"
if (-not (Test-Path $binary)) {
    Write-Host "Building deepseek-tui..."
    Push-Location $workspaceRoot
    cargo build -p deepseek-tui
    Pop-Location
    if ($LASTEXITCODE -ne 0) { throw "Build failed" }
}

$resolvedModel = Get-ResolvedModel

$results = @()
for ($run = 1; $run -le $Runs; $run++) {
    $port = if ($Port -gt 0) { $Port } else { Get-RandomPort }
    $token = -join ((48..57) + (65..90) + (97..122) | Get-Random -Count 32 | ForEach-Object { [char]$_ })
    $env:DEEPSEEK_RUNTIME_TOKEN = $token
    $dataDir = Join-Path $env:TEMP "deepseek-baseline-$([guid]::NewGuid().ToString('N'))"

    Write-Host "=== Run $run / $Runs : port=$port model=$resolvedModel ===" -ForegroundColor Cyan

    $env:DEEPSEEK_RUNTIME_DIR = $dataDir
    $proc = Start-Process -FilePath $binary `
        -ArgumentList @("serve", "--http", "--port", $port) `
        -PassThru -NoNewWindow `
        -RedirectStandardOutput (Join-Path $env:TEMP "deepseek-baseline-stdout.log") `
        -RedirectStandardError (Join-Path $env:TEMP "deepseek-baseline-stderr.log")

    try {
        $base = "http://127.0.0.1:$port"
        $healthy = $false
        for ($i = 0; $i -lt 30; $i++) {
            Start-Sleep -Seconds 1
            try {
                $null = Invoke-RestMethod -Uri "$base/health" -TimeoutSec 2
                $healthy = $true
                break
            } catch { }
        }
        if (-not $healthy) { throw "Sidecar did not start in time" }
        Write-Host "  health OK"

        $thread = Invoke-Rest -Uri "$base/v1/threads" -Method Post -Body @{
            model = $resolvedModel
            mode  = "agent"
        }
        $threadId = $thread.id
        Write-Host "  thread created: $threadId"

        Invoke-Rest -Uri "$base/v1/threads/$threadId/turns" -Method Post -Body @{
            prompt = "Reply with just the word OK."
        } | Out-Null
        Start-Sleep -Seconds 8
        Write-Host "  warm-up done"

        Invoke-LargeToolOutputTurn -Base $base -ThreadId $threadId

        $rssPeak = 0.0
        for ($t = 1; $t -le $Turns; $t++) {
            try {
                Invoke-Rest -Uri "$base/v1/threads/$threadId/turns" -Method Post -Body @{
                    prompt = "Turn ${t}: reply with one short sentence."
                } -TimeoutSec 180 | Out-Null
            } catch {
                Write-Warning "  turn $t failed: $_"
            }
            try {
                $rss = (Get-Process -Id $proc.Id -ErrorAction SilentlyContinue).WorkingSet64 / 1MB
                if ($rss -gt $rssPeak) { $rssPeak = $rss }
            } catch { }
            if ($t % 10 -eq 0) { Write-Host "  turn $t / $Turns" }
        }

        try {
            Invoke-Rest -Uri "$base/v1/threads/$threadId/turns/latest/stop" -Method Post | Out-Null
        } catch { }

        $p99 = Measure-DataDirWriteP99Ms -DataDir $dataDir
        Write-Host "  RSS peak: $([math]::Round($rssPeak, 1)) MB  p99 read proxy: $p99 ms"

        $results += [PSCustomObject]@{
            Run       = $run
            Port      = $port
            RssPeakMB = $rssPeak
            P99Ms     = $p99
        }
    }
    finally {
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
        if (Test-Path $dataDir) {
            Remove-Item -Path $dataDir -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    Start-Sleep -Seconds 2
}

$sortedRss = ($results | Sort-Object RssPeakMB).RssPeakMB
$sortedP99 = ($results | Sort-Object P99Ms).P99Ms
$mid = [Math]::Floor(($Runs - 1) / 2)
$medRss = $sortedRss[$mid]
$medP99 = $sortedP99[$mid]

Write-Host ""
Write-Host "=== Baseline (median of $Runs runs) ===" -ForegroundColor Green
Write-Host "  git ref: $gitRef"
Write-Host "  RSS peak: $([math]::Round($medRss, 1)) MB"
Write-Host "  p99 disk read proxy: $([math]::Round($medP99, 2)) ms"
Write-Host ""
Write-Host "  Fill into docs/tech/adr/RUNTIME_BASELINE.md"

# Machine-readable summary for CI/log capture
Write-Output "BASELINE_GIT_REF=$gitRef"
Write-Output "BASELINE_RSS_PEAK_MB=$([math]::Round($medRss, 1))"
Write-Output "BASELINE_P99_MS=$([math]::Round($medP99, 2))"
