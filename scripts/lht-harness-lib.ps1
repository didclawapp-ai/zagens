# Shared helpers for LHT harness headless tests (lht-harness-smoke.ps1, lht-harness-run.ps1).

function Get-LhtHarnessTempRoot {
    # Linux/pwsh CI often has no $env:TEMP; GetTempPath() honors TMPDIR and falls back to /tmp.
    return [System.IO.Path]::GetTempPath().TrimEnd('\', '/')
}

function Get-LhtHarnessRepoRoot {
    param([string]$ScriptRoot)
    return (Resolve-Path (Join-Path $ScriptRoot "..")).Path
}

function Get-LhtHarnessUtilPy {
    param([string]$RepoRoot)
    return Join-Path $RepoRoot "scripts\lht_harness_util.py"
}

function Get-LhtHarnessRandomPort {
    $listener = New-Object System.Net.Sockets.TcpListener([System.Net.IPAddress]::Loopback, 0)
    $listener.Start()
    $port = $listener.LocalEndpoint.Port
    $listener.Stop()
    return $port
}

function Invoke-LhtHarnessRest {
    param(
        [string]$Uri,
        [string]$Method = "GET",
        $Body,
        [int]$TimeoutSec = 120
    )
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
    if ($null -ne $Body) {
        $params["Body"] = ($Body | ConvertTo-Json -Depth 20 -Compress)
    }
    return Invoke-RestMethod @params
}

function Import-LhtHarnessDotEnv {
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

function Resolve-LhtHarnessApiKey {
    $zagens = Join-Path $env:USERPROFILE ".zagens\config.toml"
    $legacy = Join-Path $env:USERPROFILE ".deepseek\config.toml"
    foreach ($configPath in @($zagens, $legacy)) {
        if (-not (Test-Path $configPath)) { continue }
        $content = Get-Content -Path $configPath -Raw -ErrorAction SilentlyContinue
        if (-not $content) { continue }
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
    }
    return $null
}

function Test-LhtHarnessLhtStrictSetting {
    $settingsPaths = @(
        (Join-Path $env:USERPROFILE ".zagens\settings.toml"),
        (Join-Path $env:USERPROFILE ".deepseek\settings.toml")
    )
    foreach ($path in $settingsPaths) {
        if (-not (Test-Path $path)) { continue }
        $raw = Get-Content -Path $path -Raw -ErrorAction SilentlyContinue
        if ($raw -match '(?m)^\s*lht_composer_mode\s*=\s*"(strict|off)"\s*$') {
            return $true
        }
        if ($raw -match '(?m)^\s*lht_strict\s*=\s*true\s*$') {
            return $true
        }
    }
    return $false
}

function Get-LhtHarnessRuntimeBinary {
    param(
        [string]$RepoRoot,
        [switch]$SkipBuild
    )
    $candidates = @(
        (Join-Path $RepoRoot "target\release\zagens-runtime.exe"),
        (Join-Path $RepoRoot "target\release\deepseek-runtime.exe"),
        (Join-Path $RepoRoot "target\release\zagens-runtime"),
        (Join-Path $RepoRoot "target\release\deepseek-runtime")
    )
    foreach ($path in $candidates) {
        if (Test-Path $path) { return $path }
    }
    if ($SkipBuild) {
        throw "Runtime binary not found (zagens-runtime / deepseek-runtime). Build with: cargo build -p zagens-cli --release"
    }
    Write-Host "Building zagens-runtime (release)..." -ForegroundColor DarkGray
    Push-Location $RepoRoot
    try {
        cargo build -p zagens-cli --release
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    } finally {
        Pop-Location
    }
    return Get-LhtHarnessRuntimeBinary -RepoRoot $RepoRoot -SkipBuild
}

function Merge-LhtHarnessConfig {
    param(
        [string]$UtilPy,
        [string]$Profile,
        [string]$OutPath,
        [string]$BaseConfig = ""
    )
    $args = @(
        $UtilPy, "merge-config",
        "--profile", $Profile,
        "--out", $OutPath
    )
    if ($BaseConfig) {
        $args += @("--base", $BaseConfig)
    }
    $out = & python @args 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "merge-config failed: $out"
    }
    return $OutPath
}

function Get-LhtHarnessResolvedModel {
    param([string]$Model)
    if ($Model) { return $Model }
    if ($env:DEEPSEEK_MODEL) { return $env:DEEPSEEK_MODEL }
    return "deepseek-chat"
}

function Get-LhtHarnessGitSha {
    param([string]$RepoRoot)
    $sha = git -C $RepoRoot rev-parse --short HEAD 2>$null
    if ($sha) { return $sha.Trim() }
    return "unknown"
}

function New-LhtHarnessEphemeralWorkspace {
    param([string]$Prefix = "lht-harness-ws")
    $ws = Join-Path (Get-LhtHarnessTempRoot) "$Prefix-$([guid]::NewGuid().ToString('N'))"
    New-Item -ItemType Directory -Path $ws -Force | Out-Null
    return $ws
}

function Resolve-LhtHarnessFixturePath {
    param(
        [string]$RepoRoot,
        [string]$RelativeOrAbsolute
    )
    if ([string]::IsNullOrWhiteSpace($RelativeOrAbsolute)) { return $null }
    if ([System.IO.Path]::IsPathRooted($RelativeOrAbsolute)) {
        return $RelativeOrAbsolute
    }
    $norm = ($RelativeOrAbsolute -replace '/', [System.IO.Path]::DirectorySeparatorChar)
    $candidates = @(
        (Join-Path $RepoRoot $norm),
        (Join-Path $RepoRoot (($norm -replace '^docs\\harness\\fixtures\\', 'fixtures\harness\')))
    )
    foreach ($c in $candidates) {
        if (Test-Path $c) { return $c }
    }
    return $candidates[0]
}

function Copy-LhtHarnessWorkspaceSeed {
    param(
        [string]$RepoRoot,
        [string]$Workspace,
        [string]$SeedRelativeOrAbsolute
    )
    if ([string]::IsNullOrWhiteSpace($SeedRelativeOrAbsolute)) { return }
    $seedPath = Resolve-LhtHarnessFixturePath -RepoRoot $RepoRoot -RelativeOrAbsolute $SeedRelativeOrAbsolute
    if (-not (Test-Path $seedPath)) {
        throw "workspace_seed not found: $seedPath"
    }
    Get-ChildItem -Path $seedPath -Force | ForEach-Object {
        Copy-Item -Path $_.FullName -Destination $Workspace -Recurse -Force
    }
}

function Start-LhtHarnessSidecar {
    param(
        [string]$Binary,
        [int]$Port,
        [string]$ConfigPath,
        [string]$StderrLog,
        [string]$StdoutLog
    )
    $token = -join ((48..57) + (65..90) + (97..122) | Get-Random -Count 32 | ForEach-Object { [char]$_ })
    $env:DEEPSEEK_RUNTIME_TOKEN = $token
    $proc = Start-Process -FilePath $Binary `
        -ArgumentList @("--port", "$Port", "--config", $ConfigPath) `
        -PassThru -NoNewWindow `
        -RedirectStandardOutput $StdoutLog `
        -RedirectStandardError $StderrLog
    return [PSCustomObject]@{
        Process = $proc
        Token   = $token
        Port    = $Port
        Base    = "http://127.0.0.1:$Port"
    }
}

function Wait-LhtHarnessSidecarHealthy {
    param(
        [string]$Base,
        [int]$MaxWaitSec = 30
    )
    for ($i = 0; $i -lt $MaxWaitSec; $i++) {
        Start-Sleep -Seconds 1
        try {
            $null = Invoke-RestMethod -Uri "$Base/health" -TimeoutSec 2
            return $true
        } catch { }
    }
    return $false
}

function Stop-LhtHarnessSidecar {
    param($Process)
    if ($null -eq $Process) { return }
    Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
}

function Join-LhtHarnessApiUri {
    param(
        [string]$Base,
        [string]$RelativePath
    )
    return "$($Base.TrimEnd('/'))/$($RelativePath.TrimStart('/'))"
}

function Get-LhtHarnessLatestTurnStatus {
    param([string]$Base, [string]$ThreadId)
    $detail = Invoke-LhtHarnessRest -Uri (Join-LhtHarnessApiUri -Base $Base -RelativePath "v1/threads/$ThreadId") -Method GET
    if (-not $detail.turns -or $detail.turns.Count -eq 0) { return $null, $detail }
    return ([string]$detail.turns[-1].status), $detail
}

function Wait-LhtHarnessTurnIdle {
    param(
        [string]$Base,
        [string]$ThreadId,
        [int]$TimeoutSec = 900
    )
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $status, $null = Get-LhtHarnessLatestTurnStatus -Base $Base -ThreadId $ThreadId
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

function Invoke-LhtHarnessThreadTurn {
    param(
        [string]$Base,
        [string]$ThreadId,
        [string]$Prompt,
        [int]$TurnTimeoutSec = 900
    )
    $resp = Invoke-LhtHarnessRest -Uri (Join-LhtHarnessApiUri -Base $Base -RelativePath "v1/threads/$ThreadId/turns") -Method Post -Body @{
        prompt = $Prompt
    } -TimeoutSec 120
    $turnId = $null
    if ($resp.turn) { $turnId = $resp.turn.id }
    elseif ($resp.id) { $turnId = $resp.id }
    try {
        return Wait-LhtHarnessTurnIdle -Base $Base -ThreadId $ThreadId -TimeoutSec $TurnTimeoutSec
    } catch {
        if ($turnId) {
            try {
                Invoke-LhtHarnessRest -Uri (Join-LhtHarnessApiUri -Base $Base -RelativePath "v1/threads/$ThreadId/turns/$turnId/interrupt") -Method Post | Out-Null
                Wait-LhtHarnessTurnIdle -Base $Base -ThreadId $ThreadId -TimeoutSec 120 | Out-Null
            } catch { }
        }
        throw
    }
}

function Get-LhtHarnessTaskGraph {
    param([string]$Base, [string]$ThreadId)
    return Invoke-LhtHarnessRest -Uri (Join-LhtHarnessApiUri -Base $Base -RelativePath "v1/threads/$ThreadId/harness/task-graph") -Method GET
}

function Test-LhtHarnessRequires {
    param([string[]]$Requires)
    $missing = @()
    foreach ($tool in $Requires) {
        if (-not $tool) { continue }
        $found = $false
        if ($tool -eq "bash") {
            $found = $null -ne (Get-Command bash -ErrorAction SilentlyContinue)
        } elseif ($tool -eq "redis-cli") {
            if (Get-Command redis-cli -ErrorAction SilentlyContinue) { $found = $true }
            elseif (Test-Path "C:\Program Files\Redis\redis-cli.exe") { $found = $true }
        } else {
            $found = $null -ne (Get-Command $tool -ErrorAction SilentlyContinue)
        }
        if (-not $found) { $missing += $tool }
    }
    return $missing
}

function Add-LhtHarnessToolPath {
    $redisDir = "C:\Program Files\Redis"
    if ((Test-Path "$redisDir\redis-cli.exe") -and ($env:PATH -notlike "*$redisDir*")) {
        $env:PATH = "$redisDir;$env:PATH"
    }
}

function Get-LhtHarnessBashExe {
    if ($env:OS -notmatch "Windows" -and -not $IsWindows) { return "bash" }
    foreach ($path in @(
            "C:\Program Files\Git\bin\bash.exe",
            "C:\Program Files (x86)\Git\bin\bash.exe"
        )) {
        if (Test-Path $path) { return $path }
    }
    $cmd = Get-Command bash -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    return $null
}

function Invoke-LhtHarnessOracle {
    param(
        [string]$Workspace,
        [string]$OracleCmd,
        [string[]]$OracleArgv,
        [int]$TimeoutSec = 600
    )
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    if ($OracleArgv -and $OracleArgv.Count -gt 0) {
        $p = Start-Process -FilePath $OracleArgv[0] `
            -ArgumentList $OracleArgv[1..($OracleArgv.Count - 1)] `
            -WorkingDirectory $Workspace `
            -PassThru -NoNewWindow `
            -Wait -RedirectStandardOutput ([System.IO.Path]::GetTempFileName()) `
            -RedirectStandardError ([System.IO.Path]::GetTempFileName())
        $code = $p.ExitCode
        $cmdDisplay = ($OracleArgv -join " ")
    } elseif ($OracleCmd) {
        if ($env:OS -match "Windows" -or $IsWindows) {
            $bash = Get-LhtHarnessBashExe
            if ($OracleCmd -match '^\s*bash\s+' -and $bash) {
                $script = $OracleCmd -replace '^\s*bash\s+', ''
                $p = Start-Process -FilePath $bash `
                    -ArgumentList @("-lc", $script) `
                    -WorkingDirectory $Workspace `
                    -PassThru -NoNewWindow -Wait
            } else {
                $p = Start-Process -FilePath "cmd.exe" `
                    -ArgumentList @("/c", $OracleCmd) `
                    -WorkingDirectory $Workspace `
                    -PassThru -NoNewWindow -Wait
            }
        } else {
            $p = Start-Process -FilePath "bash" `
                -ArgumentList @("-lc", $OracleCmd) `
                -WorkingDirectory $Workspace `
                -PassThru -NoNewWindow -Wait
        }
        $code = $p.ExitCode
        $cmdDisplay = $OracleCmd
    } else {
        throw "Task has no oracle_cmd or oracle_argv"
    }
    $sw.Stop()
    return [PSCustomObject]@{
        cmd           = $cmdDisplay
        exit_code     = $code
        passed        = ($code -eq 0)
        duration_sec  = [Math]::Round($sw.Elapsed.TotalSeconds, 2)
    }
}

function Get-LhtHarnessProbeParsed {
    param(
        [string]$UtilPy,
        [string]$StderrLog
    )
    if (-not (Test-Path $StderrLog)) {
        return @{ summary = @{}; events = @() }
    }
    $json = & python $UtilPy parse-probe --file $StderrLog 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Warning "parse-probe failed: $json"
        return @{ summary = @{}; events = @() }
    }
    return ($json | ConvertFrom-Json)
}

function Write-LhtHarnessJsonlLine {
    param(
        [string]$Path,
        [hashtable]$Record
    )
    $dir = Split-Path -Parent $Path
    if ($dir -and -not (Test-Path $dir)) {
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
    }
    $utf8NoBom = New-Object System.Text.UTF8Encoding $false
    [System.IO.File]::AppendAllText($Path, ($Record | ConvertTo-Json -Depth 20 -Compress) + "`n", $utf8NoBom)
}

function Get-LhtHarnessOutcomeClass {
    param(
        $OraclePassed,
        [string]$TurnStatus,
        [hashtable]$TaskGraph,
        [hashtable]$ProbeSummary,
        [bool]$HarnessOk,
        [bool]$Infra,
        [bool]$TimedOut
    )
    if ($Infra) { return "infra" }
    if ($TimedOut) { return "timeout" }
    if (-not $HarnessOk) { return "harness_misconfig" }
    $incomplete = [bool]$TaskGraph.incomplete
    $openItems = 0
    if ($null -ne $TaskGraph.open_items) { $openItems = [int]$TaskGraph.open_items }
    $completionPct = 0
    if ($null -ne $TaskGraph.completion_pct) { $completionPct = [int]$TaskGraph.completion_pct }
    if ($OraclePassed -eq $true) { return "ok" }
    if ($OraclePassed -eq $false) {
        if (-not $incomplete -and $openItems -eq 0 -and $completionPct -ge 100) {
            return "false_green"
        }
        $graphCompleteSkips = 0
        if ($null -ne $ProbeSummary.gate_skip_graph_complete) {
            $graphCompleteSkips = [int]$ProbeSummary.gate_skip_graph_complete
        }
        if ($graphCompleteSkips -gt 0 -and $openItems -eq 0) {
            return "harness_regression"
        }
        if ($TurnStatus -eq "completed" -and $openItems -gt 0) {
            return "harness_regression"
        }
        return "task_failed"
    }
    return "infra"
}

function Get-LhtHarnessExpectedLhtEnabled {
    param([string]$Profile)
    if ($Profile -in @("harness_off", "lht_off")) { return $false }
    return $true
}
