# Phase 0.4 — collect H2 baseline metrics (maintainer-private archive).
# Usage: powershell -File scripts/harness/collect-baseline-metrics.ps1 [-Output path]
param(
    [string]$Output = ""
)

$ErrorActionPreference = "Stop"
$Root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
if (-not $Output) {
    $Output = Join-Path $Root "doc_Private\docs\metrics\baseline-2026-H2.json"
}
$OutDir = Split-Path $Output -Parent
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$ReplayDir = Join-Path $Root "fixtures\harness\kernel-v3-replay"
$GateCandidates = @(
    (Join-Path $Root "docs\harness\fixtures\microstack-completion-gate.toml"),
    (Join-Path $Root "fixtures\harness\microstack-completion-gate.toml")
)
$GateFixture = $GateCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $GateFixture) { $GateFixture = $GateCandidates[0] }
$ReplayCount = 0
if (Test-Path $ReplayDir) {
    $ReplayCount = @(Get-ChildItem -Path $ReplayDir -Filter "*.json" -File).Count
}

$ToolsJson = "null"
$SeqJson = "null"
$ZagensExe = Join-Path $Root "target\debug\zagens.exe"
$RawTools = $null
if (Test-Path $ZagensExe) {
    $RawTools = & $ZagensExe doctor --tools --json 2>$null
} elseif (Get-Command zagens -ErrorAction SilentlyContinue) {
    $RawTools = & zagens doctor --tools --json 2>$null
}
if ($RawTools) {
    $trimmed = $RawTools.Trim()
    if ($trimmed.StartsWith("{")) {
        # Embed raw JSON — avoid ConvertTo-Json mangling Unicode arrows (→) in tool_sequences.
        $ToolsJson = $trimmed
        if ($trimmed -match '"tool_sequences"\s*:\s*(\{.*\})\s*\}\s*$') {
            $SeqJson = $Matches[1]
        }
    }
}

$GeneratedAt = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
$GateRel = $GateFixture.Substring($Root.Length + 1) -replace '\\', '/'
$GatePresent = Test-Path $GateFixture

$Doc = @"
{
  "schema": "zagens-h2-baseline-v0",
  "generated_at": "$GeneratedAt",
  "phase": "0.4",
  "golden_replay": {
    "path": "fixtures/harness/kernel-v3-replay/",
    "fixture_json_count": $ReplayCount,
    "note": "Run kernel replay CI separately; this snapshot counts fixture files only."
  },
  "harness_fixtures": {
    "microstack_completion_gate": "$GateRel",
    "present": $($GatePresent.ToString().ToLower())
  },
  "historical_sessions": {
    "session_ids": [],
    "note": "Maintainer: append redacted session IDs privately before formal Phase gate."
  },
  "process_metrics": {
    "avg_rework_rounds_per_task": null,
    "verify_self_heal_rate": null,
    "tool_misuse_rate": null,
    "stage_gate_false_positive_rate": null,
    "first_turn_tool_schema_tokens": null,
    "note": "Populate after T1 aggregation matures; tool telemetry seeds from tools section."
  },
  "tools_telemetry": $ToolsJson,
  "tool_sequences": $SeqJson
}
"@

$utf8NoBom = New-Object System.Text.UTF8Encoding $false
[System.IO.File]::WriteAllText($Output, $Doc, $utf8NoBom)
Write-Host "Wrote baseline snapshot: $Output"
