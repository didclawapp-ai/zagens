# Kernel v2 corpus — one-shot replay runner (M0.1)
# Replays fixtures/harness/kernel-v2-corpus/scenarios.toml against a fresh sidecar
# per scenario, captures the persisted event stream, and produces per-step
# latency JSON via scripts/kernel_v2_corpus_report.py.
#
# Usage:
#   .\scripts\kernel-v2-corpus-run.ps1                       # all scenarios
#   .\scripts\kernel-v2-corpus-run.ps1 -TaskId read-three-files
#   .\scripts\kernel-v2-corpus-run.ps1 -Repeat 3 -RunLabel baseline-main
#   .\scripts\kernel-v2-corpus-run.ps1 -RunLabel dag-smoke -ToolsScheduler dag
#   .\scripts\kernel-v2-corpus-run.ps1 -DryRun

param(
    [string]$TaskSpec = "",
    [string]$TaskId = "",
    [string]$Profile = "harness_off",
    [int]$Repeat = 1,
    [string]$Model = "",
    [int]$TurnTimeoutSec = 0,
    [string]$OutDir = "results/kernel-v2-corpus",
    [string]$RunLabel = "",
    [string]$ToolsScheduler = "",
    [string]$ToolsPolicy = "",
    [switch]$DryRun,
    [switch]$KeepWorkspace,
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
. (Join-Path $scriptDir "lht-harness-lib.ps1")
$repoRoot = Get-LhtHarnessRepoRoot -ScriptRoot $scriptDir

if (-not $TaskSpec) {
    $TaskSpec = Join-Path $repoRoot "fixtures\harness\kernel-v2-corpus\scenarios.toml"
}
$TaskSpec = (Resolve-Path $TaskSpec).Path

Import-LhtHarnessDotEnv -Root $repoRoot

if (-not $DryRun -and -not $env:DEEPSEEK_API_KEY) {
    $key = Resolve-LhtHarnessApiKey
    if ($key) { $env:DEEPSEEK_API_KEY = $key }
}
if (-not $DryRun -and -not $env:DEEPSEEK_API_KEY) {
    $baseCfg = Join-Path $env:USERPROFILE ".zagens\config.toml"
    if (-not (Test-Path $baseCfg)) {
        $baseCfg = Join-Path $env:USERPROFILE ".deepseek\config.toml"
    }
    if (Test-Path $baseCfg) {
        Write-Host "DEEPSEEK_API_KEY not in env; continuing — runtime may resolve API key from OS keyring via merged config." -ForegroundColor DarkYellow
    } else {
        Write-Error "DEEPSEEK_API_KEY is not set. Export it, add to .env, or pass -DryRun."
        exit 1
    }
}

$utilPy = Get-LhtHarnessUtilPy -RepoRoot $repoRoot
$gitSha = Get-LhtHarnessGitSha -RepoRoot $repoRoot
$model = Get-LhtHarnessResolvedModel -Model $Model

if (-not $RunLabel) {
    $RunLabel = (Get-Date).ToUniversalTime().ToString("yyyyMMdd-HHmmss")
}

$loadArgs = @($utilPy, "load-tasks", "--spec", $TaskSpec)
if ($TaskId) { $loadArgs += @("--task-id", $TaskId) }
$tasksJson = & python @loadArgs 2>&1
if ($LASTEXITCODE -ne 0) { throw "load-tasks failed: $tasksJson" }
# PS 5.1 quirk: ConvertFrom-Json emits a JSON array as a single Object[] item;
# assign first, then wrap, so the array enumerates per scenario.
$parsed = $tasksJson | ConvertFrom-Json
$tasks = @($parsed)
if ($tasks.Count -eq 0) { throw "No scenarios loaded from $TaskSpec" }

if ($DryRun) {
    Write-Host "=== Dry run (kernel-v2 corpus) ===" -ForegroundColor Yellow
    Write-Host "  scenarios: $($tasks.Count)"
    foreach ($t in $tasks) {
        $lt = if ($t.long_thinking) { " long_thinking" } else { "" }
        Write-Host "  - $($t.id) shape=$($t.batch_shape)$lt seed=$($t.workspace_seed)"
    }
    exit 0
}

$binary = Get-LhtHarnessRuntimeBinary -RepoRoot $repoRoot -SkipBuild:$SkipBuild
$runDir = Join-Path (Join-Path $repoRoot $OutDir) $RunLabel
New-Item -ItemType Directory -Path $runDir -Force | Out-Null
$jsonlPath = Join-Path $runDir "runs.jsonl"

$configPath = Join-Path $runDir "config-$Profile.toml"
Merge-LhtHarnessConfig -UtilPy $utilPy -Profile $Profile -OutPath $configPath | Out-Null
if ($ToolsScheduler) {
    $sched = $ToolsScheduler.Trim().ToLower()
    if ($sched -notin @("legacy", "shadow", "dag")) {
        throw "Invalid -ToolsScheduler '$ToolsScheduler' (expected legacy, shadow, or dag)"
    }
    if (-not (Select-String -Path $configPath -Pattern '^\[tools\]' -Quiet)) {
        Add-Content -Path $configPath -Value "`n[tools]"
    }
    Add-Content -Path $configPath -Value "scheduler = `"$sched`""
}
if ($ToolsPolicy) {
    $pol = $ToolsPolicy.Trim().ToLower()
    if ($pol -notin @("legacy", "shadow", "engine")) {
        throw "Invalid -ToolsPolicy '$ToolsPolicy' (expected legacy, shadow, or engine)"
    }
    if (-not (Select-String -Path $configPath -Pattern '^\[tools\]' -Quiet)) {
        Add-Content -Path $configPath -Value "`n[tools]"
    }
    Add-Content -Path $configPath -Value "policy = `"$pol`""
}
$configDigest = (Get-FileHash -Path $configPath -Algorithm SHA256).Hash.Substring(0, 16)
# Track what was explicitly requested. "default" means we rely on the runtime default.
# Scheduler default is "shadow" (M4 bake), policy default is "engine" (M3 bake complete).
$toolsSchedulerLabel = if ($ToolsScheduler) { $ToolsScheduler.Trim().ToLower() } else { "default(shadow)" }
$toolsPolicyLabel    = if ($ToolsPolicy)    { $ToolsPolicy.Trim().ToLower()    } else { "default(engine)" }

Write-Host "=== Kernel v2 corpus run ===" -ForegroundColor Cyan
Write-Host "  label:    $RunLabel"
Write-Host "  profile:  $Profile"
Write-Host "  scenarios:$($tasks.Count) x repeat $Repeat"
Write-Host "  model:    $model"
Write-Host "  scheduler:$toolsSchedulerLabel"
Write-Host "  policy:   $toolsPolicyLabel"
Write-Host "  out:      $runDir"

function Save-CorpusEventStream {
    param([string]$Base, [string]$ThreadId, [string]$OutFile)
    $uri = Join-LhtHarnessApiUri -Base $Base -RelativePath "v1/threads/$ThreadId/events?since_seq=0&replay_only=1"
    $headers = @{}
    if ($env:DEEPSEEK_RUNTIME_TOKEN) {
        $headers["Authorization"] = "Bearer $env:DEEPSEEK_RUNTIME_TOKEN"
    }
    # replay_only=1 closes the SSE stream after the persisted backlog, so a
    # plain buffered GET captures the complete event history.
    $resp = Invoke-WebRequest -Uri $uri -Headers $headers -TimeoutSec 120 -UseBasicParsing
    $utf8NoBom = New-Object System.Text.UTF8Encoding $false
    [System.IO.File]::WriteAllText($OutFile, $resp.Content, $utf8NoBom)
}

function Probe-KernelShadowStats {
    param([string]$Base)
    try {
        $uri = Join-LhtHarnessApiUri -Base $Base -RelativePath "v1/runtime/kernel-shadow"
        return Invoke-LhtHarnessRest -Uri $uri -Method GET -TimeoutSec 30
    } catch {
        Write-Warning "kernel-shadow probe failed: $_"
        return $null
    }
}

$runIndex = 0
foreach ($task in $tasks) {
    $taskTimeout = $TurnTimeoutSec
    if ($taskTimeout -le 0 -and $task.turn_timeout_sec) { $taskTimeout = [int]$task.turn_timeout_sec }
    if ($taskTimeout -le 0) { $taskTimeout = 900 }

    for ($r = 1; $r -le $Repeat; $r++) {
        $runIndex++
        Write-Host ""
        Write-Host "== [$runIndex] scenario=$($task.id) shape=$($task.batch_shape) repeat=$r ==" -ForegroundColor Cyan

        $port = Get-LhtHarnessRandomPort
        $dataDir = Join-Path (Get-LhtHarnessTempRoot) "kv2-corpus-rt-$([guid]::NewGuid().ToString('N'))"
        $stderrLog = Join-Path $runDir "stderr-$($task.id)-r$r.log"
        $stdoutLog = Join-Path $runDir "stdout-$($task.id)-r$r.log"
        $eventsFile = Join-Path $runDir "events-$($task.id)-r$r.sse"
        $threadFile = Join-Path $runDir "thread-$($task.id)-r$r.json"
        $workspace = New-LhtHarnessEphemeralWorkspace -Prefix "kv2-corpus"
        if ($task.workspace_seed) {
            Copy-LhtHarnessWorkspaceSeed -RepoRoot $repoRoot -Workspace $workspace -SeedRelativeOrAbsolute $task.workspace_seed
        }
        if ($task.git_init) {
            # Seed a deterministic git history so read-only git commands have output.
            # Git on Windows writes CRLF advisories to stderr; with
            # $ErrorActionPreference = 'Stop' that becomes a terminating error unless
            # stderr is merged and discarded (2>$null alone is not enough).
            Push-Location $workspace
            try {
                $prevEap = $ErrorActionPreference
                $ErrorActionPreference = 'Continue'
                git init --quiet 2>&1 | Out-Null
                git add -A 2>&1 | Out-Null
                git -c user.name="corpus" -c user.email="corpus@localhost" commit --quiet -m "corpus seed" 2>&1 | Out-Null
                $ErrorActionPreference = $prevEap
            } finally {
                Pop-Location
            }
        }

        $startedAt = (Get-Date).ToUniversalTime().ToString("o")
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        $env:DEEPSEEK_RUNTIME_DIR = $dataDir
        $sidecar = $null
        $timedOut = $false
        $infra = $false
        $turnStatus = $null
        $threadId = $null
        $kernelShadow = $null

        try {
            $sidecar = Start-LhtHarnessSidecar -Binary $binary -Port $port -ConfigPath $configPath `
                -StderrLog $stderrLog -StdoutLog $stdoutLog
            if (-not (Wait-LhtHarnessSidecarHealthy -Base $sidecar.Base)) {
                throw "Sidecar did not become healthy"
            }

            $thread = Invoke-LhtHarnessRest -Uri (Join-LhtHarnessApiUri -Base $sidecar.Base -RelativePath "v1/threads") -Method Post -Body @{
                model     = $model
                mode      = "agent"
                task_type = "code"
                workspace = $workspace
            }
            $threadId = $thread.id

            try {
                $turnStatus = Invoke-LhtHarnessThreadTurn -Base $sidecar.Base -ThreadId $threadId `
                    -Prompt $task.prompt -TurnTimeoutSec $taskTimeout
            } catch {
                if ($_.Exception.Message -match "did not finish within") { $timedOut = $true } else { throw }
            }

            Save-CorpusEventStream -Base $sidecar.Base -ThreadId $threadId -OutFile $eventsFile
            $detail = Invoke-LhtHarnessRest -Uri (Join-LhtHarnessApiUri -Base $sidecar.Base -RelativePath "v1/threads/$threadId") -Method GET
            # Always probe shadow counters: scheduler default is "shadow" (M4 bake) and
            # policy default is "engine" (M3 bake complete). Skip only when explicitly set to a
            # non-shadow mode so the bake report has data regardless of whether -ToolsScheduler
            # or -ToolsPolicy was passed.
            $skipProbe = ($ToolsScheduler -and $ToolsScheduler.Trim().ToLower() -in @("legacy","dag")) -and
                         ($ToolsPolicy    -and $ToolsPolicy.Trim().ToLower()    -in @("legacy","engine"))
            if (-not $skipProbe) {
                $kernelShadow = Probe-KernelShadowStats -Base $sidecar.Base
            }
            $utf8NoBom = New-Object System.Text.UTF8Encoding $false
            [System.IO.File]::WriteAllText($threadFile, ($detail | ConvertTo-Json -Depth 30), $utf8NoBom)
        } catch {
            $infra = $true
            Write-Warning "Run failed: $_"
        } finally {
            if ($sidecar) { Stop-LhtHarnessSidecar -Process $sidecar.Process }
            if (Test-Path $dataDir) {
                Remove-Item -Path $dataDir -Recurse -Force -ErrorAction SilentlyContinue
            }
            if (-not $KeepWorkspace -and (Test-Path $workspace)) {
                Remove-Item -Path $workspace -Recurse -Force -ErrorAction SilentlyContinue
            } elseif ($KeepWorkspace) {
                Write-Host "  workspace kept: $workspace" -ForegroundColor DarkGray
            }
        }
        $sw.Stop()

        $record = [ordered]@{
            schema_version = 1
            run_label      = $RunLabel
            run_index      = $runIndex
            repeat_index   = $r
            scenario_id    = $task.id
            batch_shape    = $task.batch_shape
            long_thinking  = [bool]$task.long_thinking
            git_sha        = $gitSha
            config_hash    = "sha256:$configDigest"
            tools_scheduler = $toolsSchedulerLabel
            tools_policy    = $toolsPolicyLabel
            model          = $model
            started_at     = $startedAt
            duration_sec   = [Math]::Round($sw.Elapsed.TotalSeconds, 1)
            thread_id      = $threadId
            turn_status    = $turnStatus
            timed_out      = $timedOut
            infra          = $infra
            events_file    = (Split-Path -Leaf $eventsFile)
            thread_file    = (Split-Path -Leaf $threadFile)
        }
        if ($kernelShadow) {
            if ($kernelShadow.policy_shadow) {
                $record.policy_shadow = $kernelShadow.policy_shadow
            }
            if ($kernelShadow.scheduler_shadow) {
                $record.scheduler_shadow = $kernelShadow.scheduler_shadow
            }
        }
        Write-LhtHarnessJsonlLine -Path $jsonlPath -Record $record
        $color = if (-not $infra -and -not $timedOut) { "Green" } else { "Yellow" }
        Write-Host "  status=$turnStatus infra=$infra timed_out=$timedOut" -ForegroundColor $color
    }
}

Write-Host ""
Write-Host "Analyzing step latency..." -ForegroundColor Cyan
& python (Join-Path $scriptDir "kernel_v2_corpus_report.py") $runDir --spec $TaskSpec `
    --assert-prefix-stability --require-fingerprints
if ($LASTEXITCODE -ne 0) {
    Write-Warning "kernel_v2_corpus_report.py failed; raw captures remain in $runDir"
    exit 1
}
Write-Host ""
Write-Host "Wrote: $(Join-Path $runDir 'report.json')" -ForegroundColor Green
