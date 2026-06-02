# LHT Harness L2-Smoke — sidecar + config profile + optional trivial turn
# Usage:
#   .\scripts\lht-harness-smoke.ps1
#   .\scripts\lht-harness-smoke.ps1 -Full   # requires DEEPSEEK_API_KEY, runs LLM turn
#   .\scripts\lht-harness-smoke.ps1 -SkipBuild

param(
    [switch]$Full,
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
. (Join-Path $scriptDir "lht-harness-lib.ps1")
$repoRoot = Get-LhtHarnessRepoRoot -ScriptRoot $scriptDir

Import-LhtHarnessDotEnv -Root $repoRoot
Add-LhtHarnessToolPath

if ($Full -and -not $env:DEEPSEEK_API_KEY) {
    $key = Resolve-LhtHarnessApiKey
    if ($key) { $env:DEEPSEEK_API_KEY = $key }
}
if ($Full -and -not $env:DEEPSEEK_API_KEY) {
    Write-Error "Full smoke requires DEEPSEEK_API_KEY (env or ~/.zagens|~/.deepseek config.toml)"
    exit 1
}

if (Test-LhtHarnessLhtStrictSetting) {
    Write-Warning "settings.toml has lht_strict=true — may override harness_off during engine spawn; close strict for profile tests."
}

$utilPy = Get-LhtHarnessUtilPy -RepoRoot $repoRoot
$binary = Get-LhtHarnessRuntimeBinary -RepoRoot $repoRoot -SkipBuild:$SkipBuild
$gitSha = Get-LhtHarnessGitSha -RepoRoot $repoRoot

Write-Host "=== LHT Harness Smoke ===" -ForegroundColor Cyan
Write-Host "  binary: $binary"
Write-Host "  git:    $gitSha"
Write-Host "  mode:   $(if ($Full) { 'full (LLM turn)' } else { 'infra only' })"

$profiles = @(
    @{ Name = "harness_off"; Expected = $false },
    @{ Name = "harness_default"; Expected = $true }
)

$failures = 0
foreach ($item in $profiles) {
    $profile = $item.Name
    $expected = $item.Expected
    $port = Get-LhtHarnessRandomPort
    $dataDir = Join-Path $env:TEMP "lht-harness-smoke-$([guid]::NewGuid().ToString('N'))"
    $configPath = Join-Path $env:TEMP "lht-harness-smoke-config-$profile.toml"
    $stderrLog = Join-Path $env:TEMP "lht-harness-smoke-stderr-$profile.log"
    $stdoutLog = Join-Path $env:TEMP "lht-harness-smoke-stdout-$profile.log"

    New-Item -ItemType Directory -Path $dataDir -Force | Out-Null
    Merge-LhtHarnessConfig -UtilPy $utilPy -Profile $profile -OutPath $configPath | Out-Null

    $env:DEEPSEEK_RUNTIME_DIR = $dataDir
    $sidecar = $null
    $workspace = $null
    try {
        Write-Host ""
        Write-Host "-- profile: $profile (expect lht_enabled=$expected) --" -ForegroundColor Yellow
        $sidecar = Start-LhtHarnessSidecar -Binary $binary -Port $port -ConfigPath $configPath `
            -StderrLog $stderrLog -StdoutLog $stdoutLog
        if (-not (Wait-LhtHarnessSidecarHealthy -Base $sidecar.Base)) {
            throw "Sidecar health check failed"
        }
        Write-Host "  health OK"

        $workspace = New-LhtHarnessEphemeralWorkspace -Prefix "lht-harness-smoke"
        $thread = Invoke-LhtHarnessRest -Uri (Join-LhtHarnessApiUri -Base $sidecar.Base -RelativePath "v1/threads") -Method Post -Body @{
            mode      = "agent"
            task_type = "code"
            workspace = $workspace
        }
        $threadId = $thread.id
        Write-Host "  thread: $threadId"

        $graph = Get-LhtHarnessTaskGraph -Base $sidecar.Base -ThreadId $threadId
        $enabled = [bool]$graph.lht_enabled
        if ($enabled -ne $expected) {
            Write-Host "  FAIL: task-graph.lht_enabled=$enabled (expected $expected)" -ForegroundColor Red
            $failures++
        } else {
            Write-Host "  PASS: task-graph.lht_enabled=$enabled" -ForegroundColor Green
        }

        if ($Full) {
            $status = Invoke-LhtHarnessThreadTurn -Base $sidecar.Base -ThreadId $threadId `
                -Prompt "Reply with exactly the word OK and nothing else." -TurnTimeoutSec 300
            Write-Host "  trivial turn status: $status" -ForegroundColor Green
        }
    } catch {
        Write-Host "  FAIL: $_" -ForegroundColor Red
        $failures++
    } finally {
        Stop-LhtHarnessSidecar -Process $sidecar.Process
        if (Test-Path $dataDir) { Remove-Item -Path $dataDir -Recurse -Force -ErrorAction SilentlyContinue }
        if ($workspace -and (Test-Path $workspace)) {
            Remove-Item -Path $workspace -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

Write-Host ""
if ($failures -gt 0) {
    Write-Host "SMOKE FAILED ($failures check(s))" -ForegroundColor Red
    exit 1
}
Write-Host "SMOKE PASS" -ForegroundColor Green
exit 0
