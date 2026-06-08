# Hooks smoke test: unit tests + example script invocations with sample stdin JSON.
$ErrorActionPreference = "Stop"
$Root = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
Set-Location $Root
Write-Host "== cargo test hooks ==" -ForegroundColor Cyan
cargo test -p zagens-cli hooks
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$HooksDir = Join-Path $Root "scripts/hooks"
$SampleContext = '{"event":"session_start","context":{"session_id":"sess_test","workspace":"C:/tmp"}}'

function Invoke-HookScript {
  param([string]$Name, [string]$Stdin, [int[]]$AllowedExit = @(0))
  $sh = Join-Path $HooksDir $Name
  if (-not (Test-Path $sh)) { throw "Missing hook script: $sh" }
  Write-Host "== $Name ==" -ForegroundColor Cyan
  $prev = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  $out = $Stdin | sh $sh 2>&1
  $code = $LASTEXITCODE
  $ErrorActionPreference = $prev
  Write-Host "exit=$code stdout=$out"
  if ($AllowedExit -notcontains $code) {
    throw "$Name exited with $code (allowed: $($AllowedExit -join ','))"
  }
}

Invoke-HookScript -Name "echo_context.sh" -Stdin $SampleContext
Invoke-HookScript -Name "shell_env.sh" -Stdin "{}"
Invoke-HookScript -Name "updated_input.sh" -Stdin "{}"
Invoke-HookScript -Name "deny_tool.sh" -Stdin '{"event":"tool_call_before","context":{"tool_name":"exec_shell"}}'
Invoke-HookScript -Name "deny_message.sh" -Stdin '{"event":"message_submit","context":{"message":"hello"}}'
Invoke-HookScript -Name "deny_message.sh" -Stdin '{"event":"message_submit","context":{"message":"BLOCK_ME"}}' -AllowedExit @(0, 2)

Write-Host "All hook smoke checks passed." -ForegroundColor Green
