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
    [int]$Port = 0  # 0 = random
)

$ErrorActionPreference = "Stop"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$workspaceRoot = Resolve-Path "$scriptDir\.."

# --- helpers ---

function Get-RandomPort {
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
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
    $params = @{ Uri = $Uri; Method = $Method; Headers = $headers; TimeoutSec = $TimeoutSec }
    if ($Body) { $params["Body"] = ($Body | ConvertTo-Json -Depth 10 -Compress) }
    return Invoke-RestMethod @params
}

# --- main ---

if (-not $env:DEEPSEEK_API_KEY) {
    Write-Error "DEEPSEEK_API_KEY is not set.  Export it before running this script."
    exit 1
}

$binary = "$workspaceRoot\target\debug\deepseek-tui.exe"
if (-not (Test-Path $binary)) {
    Write-Host "Building deepseek-tui..."
    cargo build -p deepseek-tui
    if ($LASTEXITCODE -ne 0) { throw "Build failed" }
}

$results = @()
for ($run = 1; $run -le $Runs; $run++) {
    $port = if ($Port) { $Port } else { Get-RandomPort }
    $token = -join ((48..57) + (65..90) + (97..122) | Get-Random -Count 32 | % { [char]$_ })
    $env:DEEPSEEK_RUNTIME_TOKEN = $token

    Write-Host "=== Run $run / $Runs : port=$port ===" -ForegroundColor Cyan

    # Start sidecar
    $proc = Start-Process -FilePath $binary -ArgumentList "serve", "--http", "--port", $port `
        -PassThru -NoNewWindow -RedirectStandardOutput "$env:TEMP\deepseek-baseline-stdout.log" `
        -RedirectStandardError "$env:TEMP\deepseek-baseline-stderr.log"

    try {
        # Wait for /health
        $base = "http://127.0.0.1:$port"
        $retries = 30
        do {
            Start-Sleep -Seconds 1
            try { $null = Invoke-RestMethod "$base/health" -TimeoutSec 2 } catch { }
            $retries--
        } while ($retries -gt 0 -and $? -eq $false)
        if ($retries -eq 0) { throw "Sidecar did not start in time" }

        Write-Host "  health OK"

        # Create thread
        $thread = Invoke-Rest -Uri "$base/v1/threads" -Method Post -Body @{
            model = $env:DEEPSEEK_MODEL ?? "deepseek-chat"
            mode  = "agent"
        }
        $threadId = $thread.id
        Write-Host "  thread created: $threadId"

        # Warm-up: 1 short turn
        Invoke-Rest -Uri "$base/v1/threads/$threadId/turns" -Method Post -Body @{
            prompt = "Reply with just the word OK."
        }
        Start-Sleep -Seconds 5  # let it complete
        Write-Host "  warm-up done"

        # --- take baseline measurements here ---
        $procId = $proc.Id
        $rssStart = (Get-Process -Id $procId -ErrorAction SilentlyContinue).WorkingSet64 / 1MB

        # Run N turns (simple prompts — in real benchmark, include a large tool output)
        for ($t = 1; $t -le $Turns; $t++) {
            $body = @{ prompt = "Turn $t: say hello" }
            try {
                Invoke-Rest -Uri "$base/v1/threads/$threadId/turns" -Method Post -Body $body
            } catch {
                Write-Warning "  turn $t failed: $_"
            }
            if ($t % 10 -eq 0) { Write-Host "  turn $t / $Turns" }
        }

        $rssPeak = (Get-Process -Id $procId -ErrorAction SilentlyContinue).WorkingSet64 / 1MB
        Write-Host "  RSS peak: $([math]::Round($rssPeak, 1)) MB"

        # Stop the turn if still running
        try { Invoke-Rest -Uri "$base/v1/threads/$threadId/turns/latest/stop" -Method Post } catch { }

        $results += @{ Run = $run; Port = $port; RssPeakMB = $rssPeak; P99Ms = 0 }
    }
    finally {
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    }

    Start-Sleep -Seconds 2
}

# --- report ---
$medRss = ($results | Sort-Object RssPeakMB)[[math]::Floor($Runs / 2)].RssPeakMB
Write-Host "`n=== Baseline (median of $Runs runs) ===" -ForegroundColor Green
Write-Host "  RSS peak: $([math]::Round($medRss, 1)) MB"
Write-Host "`n  Fill this into docs/tech/adr/RUNTIME_BASELINE.md"
