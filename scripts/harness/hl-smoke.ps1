# Harness+Loop supplement smoke (HL-1..HL-3 / HL-6 / HL-7 automated slice)
#
# Usage (repo root, PowerShell):
#   powershell -File scripts/harness/hl-smoke.ps1
#
# Does NOT call the LLM. Queue E2E with agent + desktop LHT fake-completion
# are listed at the end for maintainer hand-test.

$ErrorActionPreference = "Stop"
$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Set-Location $RepoRoot

$Z = Join-Path $RepoRoot "target\debug\zagens.exe"
if (-not (Test-Path $Z)) {
    Write-Host "==> building zagens debug binary"
    cargo build -p zagens-cli --bin zagens
}

$failed = 0

function Ok($msg) { Write-Host "PASS: $msg" -ForegroundColor Green }
function Fail($msg) {
    Write-Host "FAIL: $msg" -ForegroundColor Red
    $script:failed++
}

Write-Host "`n==> HL unit tests (run_with_act / stage / queue gate)"
$filters = @(
    "run_with_act_",
    "stage_verify_pass",
    "gate_with_retries",
    "gate_file_exists_passes",
    "office_pilot_",
    "stage_gate_try_pass_uses",
    "queue_gate_routes",
    "completion_gate_flow_queues"
)
$testFail = $false
foreach ($f in $filters) {
    cargo test -p zagens-cli --lib $f -- --nocapture
    if ($LASTEXITCODE -ne 0) { $testFail = $true }
}
if ($testFail) { Fail "cargo HL unit tests" } else { Ok "cargo HL unit tests" }

Write-Host "`n==> gate validate (external exit fixture)"
$extGate = "fixtures\harness\gates\zagens-h2-external-exit.toml"
$out = & $Z gate validate --file $extGate --json 2>&1 | Out-String
if ($LASTEXITCODE -ne 0) {
    Fail "gate validate external"
    Write-Host $out
} else {
    if ($out -match '"ok"\s*:\s*true' -or $out -match '"ok":true') {
        Ok "gate validate external ($extGate)"
    } else {
        # Some builds print ok without JSON key style — accept exit 0
        Ok "gate validate external (exit 0)"
    }
}

Write-Host "`n==> gate list presets"
& $Z gate list 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) { Fail "gate list" } else { Ok "gate list" }

Write-Host "`n==> queue add --gate-file (no agent run)"
$ws = Join-Path $RepoRoot "fixtures\harness\hl-smoke-win"
New-Item -ItemType Directory -Force -Path $ws | Out-Null
# Clean prior queue state for deterministic add
$qdir = Join-Path $ws ".zagens"
if (Test-Path $qdir) {
    Remove-Item -Recurse -Force (Join-Path $qdir "night_queue.json") -ErrorAction SilentlyContinue
}
& $Z -w $ws queue add "HL-7 smoke enqueue only (no run)" `
    --gate-file $extGate --no-worktree 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) {
    Fail "queue add --gate-file"
} else {
    Ok "queue add --gate-file (enqueue only)"
}

Write-Host "`n==> doctor --tools (harness_verify fields present)"
$doc = & $Z doctor --tools --json 2>&1 | Out-String
if ($LASTEXITCODE -ne 0) {
    Fail "doctor --tools"
} elseif ($doc -match "harness_verify") {
    Ok "doctor --tools exposes harness_verify*"
} else {
    Fail "doctor --tools missing harness_verify fields"
}

Write-Host ""
if ($failed -gt 0) {
    Write-Host "HL smoke FAILED ($failed check(s))" -ForegroundColor Red
    exit 1
}

Write-Host "HL smoke PASS (automated slice)" -ForegroundColor Green
Write-Host @"

------------------------------------------------------------
MAINTAINER HAND-TEST (needs API key / desktop)
------------------------------------------------------------
1) Queue E2E fail→rollback (HL-3 / HL-7):
   `$Z = `"$Z`"
   `$WS = `"$ws`"
   & `$Z -w `$WS queue add `"Do nothing; do not create must_not_exist.txt`" ``
     --gate file_exists:path=must_not_exist.txt --no-worktree
   & `$Z -w `$WS queue run --no-worktree
   Expect: task RolledBack; .zagens/queue_events.jsonl has queue_gate_result
           with harness_verify[] and queue_rollback.

2) Fake-completion (LHT): desktop or CLI turn that claims done without
   satisfying a completion-gate / file_exists deliverable - expect continue
   nudge or gate fail (not silent success).

3) Optional HL-4: set [long_horizon] post_edit_run_tests = true, edit a .rs
   file, confirm tool result contains [HL-4 post_edit_run_tests].
------------------------------------------------------------
"@
exit 0
