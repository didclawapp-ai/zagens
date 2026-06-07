# LHT Harness L2 — headless end-to-end task run (prompt from TOML, oracle pass/fail)
# Usage:
#   .\scripts\lht-harness-run.ps1 -TaskId demo3-strict-smoke -TaskSpec fixtures\harness\lht-harness-tasks.strict.toml -Profile harness_strict
#   .\scripts\lht-harness-run.ps1 -TaskSpec fixtures\harness\lht-eval-tasks.example.toml -Repeat 1
#   .\scripts\lht-harness-run.ps1 -DryRun

param(
    [string]$TaskSpec = "",
    [string]$TaskId = "",
    [string]$Profile = "harness_default",
    [int]$Repeat = 1,
    [string]$Model = "",
    [int]$TurnTimeoutSec = 0,
    [string]$OutDir = "results/lht-harness",
    [string]$RunLabel = "",
    [switch]$DryRun,
    [switch]$KeepWorkspace,
    [switch]$SkipBuild,
    [switch]$SkipOracle
)

$ErrorActionPreference = "Stop"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
. (Join-Path $scriptDir "lht-harness-lib.ps1")
$repoRoot = Get-LhtHarnessRepoRoot -ScriptRoot $scriptDir

if (-not $TaskSpec) {
    $TaskSpec = Join-Path $repoRoot "fixtures\harness\lht-eval-tasks.example.toml"
}
$TaskSpec = (Resolve-Path $TaskSpec).Path

Import-LhtHarnessDotEnv -Root $repoRoot
Add-LhtHarnessToolPath

if (-not $DryRun -and -not $env:DEEPSEEK_API_KEY) {
    $key = Resolve-LhtHarnessApiKey
    if ($key) { $env:DEEPSEEK_API_KEY = $key }
}
if (-not $DryRun -and -not $env:DEEPSEEK_API_KEY) {
    Write-Error "DEEPSEEK_API_KEY is not set. Export it or pass -DryRun."
    exit 1
}

if (Test-LhtHarnessLhtStrictSetting) {
    Write-Warning "settings.toml has lht_strict=true — engine spawn may force strict even when profile is harness_off."
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
$parsed = $tasksJson | ConvertFrom-Json
if ($parsed -is [System.Array]) {
    $tasks = @($parsed)
} else {
    $tasks = @($parsed)
}

if ($tasks.Count -eq 0) { throw "No tasks loaded from $TaskSpec" }

if ($DryRun) {
    Write-Host "=== Dry run ===" -ForegroundColor Yellow
    Write-Host "  tasks: $($tasks.Count)"
    foreach ($t in $tasks) {
        $tp = if ($t.harness_profile) { $t.harness_profile } else { $Profile }
        Write-Host "  - $($t.id) tier=$($t.tier) profile=$tp seed=$($t.workspace_seed) requires=$($t.requires -join ',')"
    }
    exit 0
}

$binary = Get-LhtHarnessRuntimeBinary -RepoRoot $repoRoot -SkipBuild:$SkipBuild
$outRoot = Join-Path $repoRoot $OutDir
$runDir = Join-Path $outRoot $RunLabel
$jsonlPath = Join-Path $runDir "runs.jsonl"
New-Item -ItemType Directory -Path $runDir -Force | Out-Null

$jsonlPath = Join-Path $runDir "runs.jsonl"
New-Item -ItemType Directory -Path $runDir -Force | Out-Null

Write-Host "=== LHT Harness Run ===" -ForegroundColor Cyan
Write-Host "  label:   $RunLabel"
Write-Host "  default profile: $Profile"
Write-Host "  tasks:   $($tasks.Count) x repeat $Repeat"
Write-Host "  model:   $model"
Write-Host "  out:     $jsonlPath"

$runIndex = 0

foreach ($task in $tasks) {
    $effectiveProfile = $Profile
    if ($task.harness_profile) { $effectiveProfile = [string]$task.harness_profile }

    $configPath = Join-Path $runDir "config-$($task.id)-$effectiveProfile.toml"
    Merge-LhtHarnessConfig -UtilPy $utilPy -Profile $effectiveProfile -OutPath $configPath | Out-Null
    $configDigest = (Get-FileHash -Path $configPath -Algorithm SHA256).Hash.Substring(0, 16)
    $expectedLht = Get-LhtHarnessExpectedLhtEnabled -Profile $effectiveProfile

    if ($effectiveProfile -match 'strict' -and (Test-LhtHarnessLhtStrictSetting)) {
        Write-Warning "Task $($task.id): settings.toml lht_strict=true stacks on harness_strict profile."
    }
    $requires = @()
    if ($task.requires) { $requires = @($task.requires) }
    $missing = Test-LhtHarnessRequires -Requires $requires
    if ($missing.Count -gt 0) {
        Write-Warning "Task $($task.id): missing tools: $($missing -join ', ') — oracle may fail with infra"
    }

    $taskTimeout = $TurnTimeoutSec
    if ($taskTimeout -le 0 -and $task.turn_timeout_sec) {
        $taskTimeout = [int]$task.turn_timeout_sec
    }
    if ($taskTimeout -le 0) { $taskTimeout = 3600 }

    for ($r = 1; $r -le $Repeat; $r++) {
        $runIndex++
        Write-Host ""
        Write-Host "== [$runIndex] task=$($task.id) repeat=$r profile=$effectiveProfile ==" -ForegroundColor Cyan

        $port = Get-LhtHarnessRandomPort
        $dataDir = Join-Path (Get-LhtHarnessTempRoot) "lht-harness-rt-$([guid]::NewGuid().ToString('N'))"
        $stderrLog = Join-Path $runDir "stderr-$($task.id)-r$r.log"
        $stdoutLog = Join-Path $runDir "stdout-$($task.id)-r$r.log"
        $workspace = New-LhtHarnessEphemeralWorkspace -Prefix "lht-harness"
        if ($task.workspace_seed) {
            Copy-LhtHarnessWorkspaceSeed -RepoRoot $repoRoot -Workspace $workspace -SeedRelativeOrAbsolute $task.workspace_seed
            Write-Host "  seed:    $($task.workspace_seed)" -ForegroundColor DarkGray
        }
        $startedAt = (Get-Date).ToUniversalTime().ToString("o")
        $sw = [System.Diagnostics.Stopwatch]::StartNew()

        $env:DEEPSEEK_RUNTIME_DIR = $dataDir
        $sidecar = $null
        $timedOut = $false
        $infra = $false
        $turnStatus = $null
        $threadId = $null
        $graph = @{}
        $oracle = $null
        $probe = @{ summary = @{}; events = @() }
        $usage = $null

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

            $graphObj = Get-LhtHarnessTaskGraph -Base $sidecar.Base -ThreadId $threadId
            $graph = @{}
            $graphObj.PSObject.Properties | ForEach-Object { $graph[$_.Name] = $_.Value }

            try {
                $turnStatus = Invoke-LhtHarnessThreadTurn -Base $sidecar.Base -ThreadId $threadId `
                    -Prompt $task.prompt -TurnTimeoutSec $taskTimeout
            } catch {
                if ($_.Exception.Message -match "did not finish within") {
                    $timedOut = $true
                } else {
                    throw
                }
            }

            if (-not $timedOut) {
                $graphObj = Get-LhtHarnessTaskGraph -Base $sidecar.Base -ThreadId $threadId
                $graph = @{}
                $graphObj.PSObject.Properties | ForEach-Object { $graph[$_.Name] = $_.Value }

                $_, $detail = Get-LhtHarnessLatestTurnStatus -Base $sidecar.Base -ThreadId $threadId
                if ($detail.turns -and $detail.turns.Count -gt 0) {
                    $last = $detail.turns[-1]
                    if ($last.usage) {
                        $usage = @{
                            input_tokens  = $last.usage.input_tokens
                            output_tokens = $last.usage.output_tokens
                        }
                    }
                }
            }

            $probeParsed = Get-LhtHarnessProbeParsed -UtilPy $utilPy -StderrLog $stderrLog
            $probe = @{
                summary = @{}
                events  = @($probeParsed.events)
            }
            if ($probeParsed.summary) {
                $probeParsed.summary.PSObject.Properties | ForEach-Object {
                    $probe.summary[$_.Name] = $_.Value
                }
            }

            if (-not $SkipOracle -and -not $timedOut) {
                if ($task.oracle_argv) {
                    $oracle = Invoke-LhtHarnessOracle -Workspace $workspace -OracleArgv @($task.oracle_argv)
                } elseif ($task.oracle_cmd) {
                    $oracle = Invoke-LhtHarnessOracle -Workspace $workspace -OracleCmd $task.oracle_cmd
                } else {
                    $infra = $true
                    $oracle = @{ cmd = ""; exit_code = -1; passed = $false; duration_sec = 0 }
                }
            } elseif ($SkipOracle) {
                $oracle = @{ cmd = "(skipped)"; exit_code = 0; passed = $true; duration_sec = 0 }
            } else {
                $oracle = @{ cmd = ""; exit_code = -1; passed = $false; duration_sec = 0 }
            }
        } catch {
            $infra = $true
            Write-Warning "Run failed: $_"
            if (-not $oracle) {
                $oracle = @{ cmd = ""; exit_code = -1; passed = $false; duration_sec = 0 }
            }
        } finally {
            Stop-LhtHarnessSidecar -Process $sidecar.Process
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
        $lhtEnabled = $false
        if ($graph.ContainsKey("lht_enabled")) { $lhtEnabled = [bool]$graph["lht_enabled"] }
        elseif ($null -ne $graph.lht_enabled) { $lhtEnabled = [bool]$graph.lht_enabled }
        $harnessOk = ($lhtEnabled -eq $expectedLht)

        $outcomeClass = Get-LhtHarnessOutcomeClass `
            -OraclePassed $(if ($null -ne $oracle.passed) { [bool]$oracle.passed } else { $null }) `
            -TurnStatus $turnStatus `
            -TaskGraph $graph `
            -ProbeSummary $probe.summary `
            -HarnessOk $harnessOk `
            -Infra $infra `
            -TimedOut $timedOut

        $passed = ($oracle.passed -eq $true) -and $harnessOk -and (-not $infra) -and (-not $timedOut)

        $record = [ordered]@{
            schema_version   = 1
            run_label        = $RunLabel
            run_index        = $runIndex
            repeat_index     = $r
            harness_profile  = $effectiveProfile
            task_id          = $task.id
            git_sha          = $gitSha
            config_hash      = "sha256:$configDigest"
            model            = $model
            started_at       = $startedAt
            duration_sec     = [Math]::Round($sw.Elapsed.TotalSeconds, 1)
            thread_id        = $threadId
            turn_status      = $turnStatus
            passed           = $passed
            oracle           = @{
                cmd          = $oracle.cmd
                exit_code    = $oracle.exit_code
                passed       = [bool]$oracle.passed
                duration_sec = $oracle.duration_sec
            }
            harness_assertions = @{
                ok       = $harnessOk
                failures = @(
                    if (-not $harnessOk) {
                        "lht_enabled=$lhtEnabled expected=$expectedLht"
                    }
                )
            }
            task_graph = @{
                completion_pct = $graph.completion_pct
                open_items     = $graph.open_items
                incomplete     = $graph.incomplete
                lht_enabled    = $graph.lht_enabled
                lht_blocked    = $graph.lht_blocked
            }
            usage          = $usage
            outcome_class  = $outcomeClass
            probe_summary  = $probe.summary
            probe_event_count = $probe.events.Count
        }

        Write-LhtHarnessJsonlLine -Path $jsonlPath -Record $record
        $color = if ($passed) { "Green" } else { "Yellow" }
        Write-Host "  outcome=$outcomeClass passed=$passed oracle_exit=$($oracle.exit_code)" -ForegroundColor $color
    }
}

Write-Host ""
Write-Host "Wrote: $jsonlPath" -ForegroundColor Green
Write-Host "Report: python scripts\lht-harness-report.py `"$jsonlPath`" -Gate"
