# One-off: move non-public docs/ entries to doc_Private/ and git-rm from docs/.
$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
Set-Location $root

$privateRel = @(
    "docs/Agent+Harness组合式编程方案.md",
    "docs/agent-reliability-craft-plan.md",
    "docs/bundled-python-plan.md",
    "docs/CODE_REVIEW_2025-05-11.md",
    "docs/craft-implementation-issues.md",
    "docs/desktop/audit-scratchpad-design.md",
    "docs/desktop/audit-scratchpad-test.md",
    "docs/desktop/auditor-subagent-design.md",
    "docs/desktop/DESKTOP_IMPLEMENTATION_PLAN.md",
    "docs/desktop/DESKTOP_IMPLEMENTATION_STEPS.md",
    "docs/desktop/DEV_NOTES.md",
    "docs/desktop/MCP_ITERATION_PLAN.md",
    "docs/desktop/multi-window-plan.md",
    "docs/desktop/SIDECAR_SUPERVISOR_HARDENING_PLAN.md",
    "docs/desktop/SYSTEM_SETTINGS_PLAN.md",
    "docs/desktop/TUI_DS_PICK_GAP_DEMO.html",
    "docs/desktop/workspace-directory-plan.md",
    "docs/edit-file-v0-improvements.md",
    "docs/edit-file-v1-improvements.md",
    "docs/edit-file-v2-improvements.md",
    "docs/harness/Agent+Harness组合式编程方案.md",
    "docs/harness/HARNESS_INTEGRATION_PROPOSAL.md",
    "docs/harness/PAPER_silent_early_stopping.md",
    "docs/harness/PARALLEL_FRESH_GENERATION.md",
    "docs/harness/fixtures/lht-label-rust-round2-checklist.md",
    "docs/harness/fixtures/lht-refactor-round2-checklist.md",
    "docs/office-doc-capability-plan.md",
    "docs/office-mode-iteration-plan.md",
    "docs/office-read-tool-plan.md",
    "docs/pptx-generation-engine-plan.md",
    "docs/retrieval-pipeline-enhancement.md",
    "docs/symbol-index-v2-improvements.md",
    "docs/symbol-index-v3-improvements.md",
    "docs/symbol-index-v4-improvements.md",
    "docs/symbol-index-v5-improvements.md",
    "docs/symbol-index-v6-improvements.md",
    "docs/symbol-index-v7-improvements.md",
    "docs/topic-memory-rust-plan.md",
    "docs/xlsx-production-plan.md",
    "docs/tech/ARCHITECTURE_BOUNDARY_ANALYSIS.md",
    "docs/tech/OPENCODE_AGENT_CORE_BENCHMARK.md",
    "docs/tech/RUNTIME_EVOLUTION_ROADMAP.md",
    "docs/tech/SUBAGENT_STABILITY_ANALYSIS.md",
    "docs/tech/TOOL_SURFACE_AUDIT.md",
    "docs/tech/WRITE_FILE_IMPROVEMENTS.md",
    "docs/tech/adr/A1_PERSIST_BLOCKING_AUDIT.md",
    "docs/tech/adr/A2_TURN_OBSERVABILITY_V1_DRAFT.md",
    "docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md",
    "docs/tech/adr/BACKLOG_ENGINE_STRUCT_IN_CORE.md",
    "docs/tech/adr/BACKLOG_LANDLOCK_ENFORCE.md",
    "docs/tech/adr/BACKLOG_RUNTIME_UNIFICATION.md",
    "docs/tech/adr/BACKLOG_STATESTORE_JSONL.md",
    "docs/tech/adr/D6_IMPLEMENTATION_PLAN.md",
    "docs/tech/adr/D6_PHASE_B_SPIKE.md",
    "docs/tech/adr/D7_HANDOFF.md",
    "docs/tech/adr/D7_PERSISTENCE_UNIFICATION.md",
    "docs/tech/adr/G2_PR5_MANUAL_SMOKE_CHECKLIST.md",
    "docs/tech/adr/HARNESS_INTEGRATION_PROPOSAL.md",
    "docs/tech/adr/IMPLEMENTATION_SUMMARY_2026-05-24.md",
    "docs/tech/adr/P2_A1_F3_SESSION_HANDOFF.md",
    "docs/tech/adr/P2_D10_UNFREEZE_RECORD.md",
    "docs/tech/adr/P2_DESKTOP_TURNLOOP_SPIKE.md",
    "docs/tech/adr/P2_G3_ENGINE_L2_SIGNOFF.md",
    "docs/tech/adr/P2_MIGRATION_SPIKE.md",
    "docs/tech/adr/P2_PR4_SESSION_HANDOFF.md",
    "docs/tech/adr/P2_PR6_TURN_LOOP_L2_MIGRATION_PLAN.md",
    "docs/tech/adr/PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md",
    "docs/tech/adr/SESSION_HANDOFF_2026-05-26.md",
    "docs/tech/adr/SESSION_HANDOFF_D16_PHASE_E.md",
    "docs/tech/adr/SESSION_HANDOFF_D6_PHASE_B.md"
)

$privateDirs = @(
    "docs/topic-memory-graph-main",
    "docs/tui"
)

foreach ($rel in $privateRel) {
    $src = Join-Path $root $rel
    if (-not (Test-Path $src)) {
        Write-Warning "skip missing: $rel"
        continue
    }
    $dst = Join-Path $root "doc_Private/$rel"
    $parent = Split-Path $dst -Parent
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    Move-Item -LiteralPath $src -Destination $dst -Force
    git rm -r --cached --ignore-unmatch -- $rel | Out-Null
    Write-Host "moved: $rel"
}

foreach ($rel in $privateDirs) {
    $src = Join-Path $root $rel
    if (-not (Test-Path $src)) {
        Write-Warning "skip missing dir: $rel"
        continue
    }
    $dst = Join-Path $root "doc_Private/$rel"
    $parent = Split-Path $dst -Parent
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    if (Test-Path $dst) { Remove-Item -Recurse -Force $dst }
    Move-Item -LiteralPath $src -Destination $dst -Force
    git rm -r --cached --ignore-unmatch -- $rel | Out-Null
    Write-Host "moved dir: $rel"
}

Write-Host "done. Public docs remain under docs/"
