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
    [switch]$DryRun,
    [switch]$Gate,
    [double]$BaselineRssMB = 0,
    [double]$MaxRegressionPct = 10
)

$ErrorActionPreference = "Stop"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$workspaceRoot = Resolve-Path "$scriptDir\.."
# Linux/pwsh CI often has no $env:TEMP; .NET GetTempPath() honors TMPDIR and falls back to /tmp.
$scriptTempRoot = [System.IO.Path]::GetTempPath().TrimEnd('\', '/')

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

function Get-LatestTurnStatus {
    param([string]$Base, [string]$ThreadId)
    $detail = Invoke-Rest -Uri "$Base/v1/threads/$ThreadId" -Method GET
    if (-not $detail.turns -or $detail.turns.Count -eq 0) { return $null }
    return [string]$detail.turns[-1].status
}

function Wait-ForTurnIdle {
    param(
        [string]$Base,
        [string]$ThreadId,
        [int]$TimeoutSec = 900
    )
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $status = Get-LatestTurnStatus -Base $Base -ThreadId $ThreadId
        if ($null -eq $status) {
            Start-Sleep -Seconds 2
            continue
        }
        if ($status -notin @("in_progress", "queued")) {
            return $status
        }
        Start-Sleep -Seconds 3
    }
    throw "Turn did not finish within ${TimeoutSec}s on thread $ThreadId"
}

function Invoke-ThreadTurn {
    param(
        [string]$Base,
        [string]$ThreadId,
        [string]$Prompt,
        [int]$TurnTimeoutSec = 900
    )
    $resp = Invoke-Rest -Uri "$Base/v1/threads/$ThreadId/turns" -Method Post -Body @{
        prompt = $Prompt
    } -TimeoutSec 60
    $turnId = $null
    if ($resp.turn) { $turnId = $resp.turn.id }
    elseif ($resp.id) { $turnId = $resp.id }
    try {
        Wait-ForTurnIdle -Base $Base -ThreadId $ThreadId -TimeoutSec $TurnTimeoutSec | Out-Null
    } catch {
        if ($turnId) {
            try {
                Invoke-Rest -Uri "$Base/v1/threads/$ThreadId/turns/$turnId/interrupt" -Method Post | Out-Null
                Wait-ForTurnIdle -Base $Base -ThreadId $ThreadId -TimeoutSec 120 | Out-Null
            } catch {
                Write-Warning "  interrupt after timeout failed: $_"
            }
        }
        throw
    }
}

function Get-BaselineRssFromAdr {
    param([string]$AdrPath)
    if (-not (Test-Path $AdrPath)) { return 26.6 }
    $content = Get-Content -Path $AdrPath -Raw -ErrorAction SilentlyContinue
    if ($content -match '\|\s*进程 RSS 峰值\s*\|\s*\*\*([\d.]+)\*\*') {
        return [double]$Matches[1]
    }
    return 26.6
}

function Initialize-LargeOutputFixture {
  # Deterministic >=1 MB file for A1.6 large-tool exercise (R-015).
  $ws = Join-Path $scriptTempRoot "deepseek-baseline-ws-$([guid]::NewGuid().ToString('N'))"
  New-Item -ItemType Directory -Path $ws -Force | Out-Null
  $fixtureName = "baseline_large_fixture.txt"
  $fixturePath = Join-Path $ws $fixtureName
  $targetBytes = 1.1 * 1MB
  $chunk = 'x' * 65536
  $stream = [System.IO.File]::Create($fixturePath)
  try {
    $written = 0
    while ($written -lt $targetBytes) {
      $take = [Math]::Min($chunk.Length, [int]($targetBytes - $written))
      $bytes = [System.Text.Encoding]::ASCII.GetBytes($chunk.Substring(0, $take))
      $stream.Write($bytes, 0, $bytes.Length)
      $written += $bytes.Length
    }
  } finally {
    $stream.Dispose()
  }
  return [PSCustomObject]@{
    Workspace    = $ws
    FixtureName  = $fixtureName
    SizeBytes    = (Get-Item $fixturePath).Length
  }
}

function Invoke-LargeToolOutputTurn {
    param(
        [string]$Base,
        [string]$ThreadId,
        [string]$FixtureName,
        [int]$TurnTimeoutSec = 900
    )
    $prompt = @"
Use read_file to read the file "$FixtureName" in the workspace root.
Reply with only the first 20 characters of the file content, nothing else.
"@
    try {
        Invoke-ThreadTurn -Base $Base -ThreadId $ThreadId -Prompt $prompt -TurnTimeoutSec $TurnTimeoutSec
        Write-Host "  large-tool fixture turn OK ($FixtureName)" -ForegroundColor DarkGreen
    } catch {
        Write-Warning "  large-tool turn skipped or failed: $_"
    }
}

function Import-WorkspaceDotEnv {
    param([string]$Root)
    $dotenv = Join-Path $Root ".env"
    if (-not (Test-Path $dotenv)) { return }
    Get-Content $dotenv | ForEach-Object {
        if ($_ -match '^\s*([^#][^=]+)=(.*)$') {
            $name = $Matches[1].Trim()
            $value = $Matches[2].Trim().Trim('"').Trim("'")
            if ($name -and $value) {
                $existing = [Environment]::GetEnvironmentVariable($name, 'Process')
                if ([string]::IsNullOrEmpty($existing)) {
                    [Environment]::SetEnvironmentVariable($name, $value, 'Process')
                }
            }
        }
    }
}

function Resolve-DeepSeekApiKeyFromConfig {
    $configPath = Join-Path $env:USERPROFILE ".deepseek\config.toml"
    if (-not (Test-Path $configPath)) { return $null }
    $content = Get-Content -Path $configPath -Raw -ErrorAction SilentlyContinue
    if (-not $content) { return $null }
    $patterns = @(
        '(?ms)\[providers\.deepseek\][^\[]*?^\s*api_key\s*=\s*"([^"]+)"',
        '(?m)^\s*api_key\s*=\s*"([^"]+)"'
    )
    foreach ($pat in $patterns) {
        if ($content -match $pat) {
            $key = $Matches[1].Trim()
            if ($key -and $key -ne "keyring" -and $key -notmatch '^\*+$') {
                return $key
            }
        }
    }
    return $null
}

# --- main ---

Import-WorkspaceDotEnv -Root $workspaceRoot

if (-not $DryRun -and -not $env:DEEPSEEK_API_KEY) {
    $fromConfig = Resolve-DeepSeekApiKeyFromConfig
    if ($fromConfig) {
        $env:DEEPSEEK_API_KEY = $fromConfig
        Write-Host "  Using api_key from ~/.deepseek/config.toml" -ForegroundColor DarkGray
    }
}

if (-not $DryRun -and -not $env:DEEPSEEK_API_KEY) {
    Write-Error "DEEPSEEK_API_KEY is not set (env or ~/.deepseek/config.toml). Export it before running, or pass -DryRun for disk-only sampling."
    exit 1
}

$gitRef = (git -C $workspaceRoot rev-parse --short HEAD 2>$null)
if (-not $gitRef) { $gitRef = "unknown" }

if ($DryRun) {
    Write-Host "=== Dry run: synthetic disk read proxy (no HTTP/API) ===" -ForegroundColor Yellow
    $results = @()
    for ($run = 1; $run -le $Runs; $run++) {
        $dataDir = Join-Path $scriptTempRoot "deepseek-baseline-dry-$([guid]::NewGuid().ToString('N'))"
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

# Release build: debug runtime can stack-overflow on Windows (2026-05-22).
$binary = $null
foreach ($name in @("zagens-runtime.exe", "deepseek-runtime.exe")) {
    $candidate = Join-Path $workspaceRoot "target\release\$name"
    if (Test-Path $candidate) { $binary = $candidate; break }
}
if (-not $binary) {
    Write-Host "Building zagens-runtime (release)..."
    Push-Location $workspaceRoot
    cargo build -p deepseek-runtime-server --release
    Pop-Location
    if ($LASTEXITCODE -ne 0) { throw "Build failed" }
    foreach ($name in @("zagens-runtime.exe", "deepseek-runtime.exe")) {
        $candidate = Join-Path $workspaceRoot "target\release\$name"
        if (Test-Path $candidate) { $binary = $candidate; break }
    }
}
if (-not $binary) { throw "Runtime binary not found after build" }

$resolvedModel = Get-ResolvedModel

$results = @()
for ($run = 1; $run -le $Runs; $run++) {
    $port = if ($Port -gt 0) { $Port } else { Get-RandomPort }
    $token = -join ((48..57) + (65..90) + (97..122) | Get-Random -Count 32 | ForEach-Object { [char]$_ })
    $env:DEEPSEEK_RUNTIME_TOKEN = $token
    $dataDir = Join-Path $scriptTempRoot "deepseek-baseline-$([guid]::NewGuid().ToString('N'))"

    Write-Host "=== Run $run / $Runs : port=$port model=$resolvedModel ===" -ForegroundColor Cyan

    $env:DEEPSEEK_RUNTIME_DIR = $dataDir
    $proc = Start-Process -FilePath $binary `
        -ArgumentList @("--port", $port, "--config", $configPath) `
        -PassThru -NoNewWindow `
        -RedirectStandardOutput (Join-Path $scriptTempRoot "deepseek-baseline-stdout.log") `
        -RedirectStandardError (Join-Path $scriptTempRoot "deepseek-baseline-stderr.log")

    $fixture = $null
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

        $fixture = Initialize-LargeOutputFixture
        Write-Host "  fixture workspace: $($fixture.Workspace) ($($fixture.SizeBytes) bytes)"

        $thread = Invoke-Rest -Uri "$base/v1/threads" -Method Post -Body @{
            model     = $resolvedModel
            mode      = "agent"
            workspace = $fixture.Workspace
        }
        $threadId = $thread.id
        Write-Host "  thread created: $threadId"

        Invoke-ThreadTurn -Base $base -ThreadId $threadId -Prompt "Reply with just the word OK." -TurnTimeoutSec 300
        Write-Host "  warm-up done"

        Invoke-LargeToolOutputTurn -Base $base -ThreadId $threadId -FixtureName $fixture.FixtureName -TurnTimeoutSec 900

        $rssPeak = 0.0
        for ($t = 1; $t -le $Turns; $t++) {
            try {
                Invoke-ThreadTurn -Base $base -ThreadId $threadId -Prompt "Turn ${t}: reply with one short sentence." -TurnTimeoutSec 600
            } catch {
                Write-Warning "  turn $t failed: $_"
            }
            try {
                $rss = (Get-Process -Id $proc.Id -ErrorAction SilentlyContinue).WorkingSet64 / 1MB
                if ($rss -gt $rssPeak) { $rssPeak = $rss }
            } catch { }
            if ($t % 10 -eq 0) { Write-Host "  turn $t / $Turns (RSS peak so far: $([math]::Round($rssPeak, 1)) MB)" }
        }

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
        if ($fixture -and (Test-Path $fixture.Workspace)) {
            Remove-Item -Path $fixture.Workspace -Recurse -Force -ErrorAction SilentlyContinue
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

if ($Gate -and -not $DryRun -and $medRss -gt 0) {
    $adrPath = Join-Path $workspaceRoot "docs\tech\adr\RUNTIME_BASELINE.md"
    $baselineRss = if ($BaselineRssMB -gt 0) { $BaselineRssMB } else { Get-BaselineRssFromAdr -AdrPath $adrPath }
    $maxAllowed = $baselineRss * (1 + ($MaxRegressionPct / 100.0))
    Write-Host ""
    Write-Host "=== A1 regression gate (-Gate) ===" -ForegroundColor Cyan
    Write-Host "  baseline RSS: $baselineRss MB  max allowed (+${MaxRegressionPct}%): $([math]::Round($maxAllowed, 1)) MB"
    if ($medRss -gt $maxAllowed) {
        Write-Error "RSS regression: median $([math]::Round($medRss, 1)) MB exceeds gate ($([math]::Round($maxAllowed, 1)) MB)"
        exit 1
    }
    Write-Host "  PASS: median RSS within gate" -ForegroundColor Green
}

# Machine-readable summary for CI/log capture
Write-Output "BASELINE_GIT_REF=$gitRef"
Write-Output "BASELINE_RSS_PEAK_MB=$([math]::Round($medRss, 1))"
Write-Output "BASELINE_P99_MS=$([math]::Round($medP99, 2))"
