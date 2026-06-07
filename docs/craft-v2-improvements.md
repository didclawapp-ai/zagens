# CRAFT V2 Improvements

**Status:** Draft  
**Based on:** craft-demo-001 field validation + code review findings  
**Date:** 2026-05-15

---

## Problem Diagnosis

CRAFT V1 (currently implemented P0–P4) splits the software development workflow into four roles, but each role still has three gaps in **capability boundaries**:

| # | Gap | Evidence | Consequence |
|---|-----|----------|-------------|
| C1 | Reviewer cannot verify compilation | Review has no `exec_shell` in `build_allowed_tools` | Incomplete review — static checks only |
| C2 | Explorer coverage has no standard | In the demo, Explorer says "done reading" and stops | May miss key files; Implementer acts on incomplete information |
| C3 | Blackboard `rounds` field is always an empty array | `write_blackboard_partition` hard-codes `"rounds": json!([])` | fix-loop iteration history is lost; no traceability |

Three additional structural limitations are out of scope for this iteration (see §4).

---

## Improvements

### C1: Add read-only shell capability to Reviewer

**Current:** Review tool list = `list_dir, read_file, grep_files, file_search, note`

**Change:** Add `exec_shell`, but constrain via system prompt to "verification commands only"

```
Review tool list: list_dir, read_file, grep_files, file_search, exec_shell, note
```

**Safety margins:**
- P4 stash snapshot is created before Implementer runs — even if Reviewer's shell is misused, code can be restored
- Reviewer's system prompt in `build_subagent_system_prompt` already constrains behavior to "review, not modify"
- No change to `exec_shell` itself — scope is one line in the tool list

**Change size:**

| File | Location | Change |
|------|----------|--------|
| `crates/tui/src/tools/subagent/mod.rs` | L3850 | Add `"exec_shell"` to Review's vec |

**Acceptance:** Dispatch Review agent → confirm it can run `cargo check -p xxx 2>&1` and receive results

---

### C2: Explorer coverage requirements

**Current:** Explorer system prompt only says "analyze code" with no "did you cover everything?" checkpoint

**Change:** Two-layer improvement

**2a — Hard output requirement in system prompt** (modify Explorer branch in `build_subagent_system_prompt`):

Add to Explorer's task-completion prompt:

```
Before completing your analysis, append a ## Coverage Report section:

- Files examined: [list every file path you read]
- Files NOT examined that may be relevant: [list paths you suspect are relevant but didn't read, with reasons]
- Confidence: [high / medium / low] — if medium or low, explain what you would need to read to reach high
```

**2b — Add fields to blackboard explorer partition** (modify `write_blackboard_partition` in `blackboard.rs`):

```json
"explorer": {
  "findings": [...],
  "impact_summary": "...",
  "files_examined": ["path1", "path2", ...],   // new
  "coverage_confidence": "high"                  // new
}
```

`files_examined` is parsed from Explorer output (match file list in `## Coverage Report` section).

**Change size:**

| File | Location | Change |
|------|----------|--------|
| `crates/tui/src/tools/subagent/mod.rs` | Explorer branch in `build_subagent_system_prompt` | +~15 lines prompt |
| `crates/tui/src/tools/subagent/blackboard.rs` | `write_blackboard_partition` | +~10 lines (parse + new fields) |

**Acceptance:** Dispatch Explorer → confirm output has non-empty `## Coverage Report` → confirm blackboard `files_examined` is populated

---

### C3: Blackboard `rounds` field records real fix-loop iterations

**Current:** Dead code in `write_blackboard_partition`:

```rust
"rounds": json!([]), // placeholder — filled by merge logic
```

This "merge logic" was never implemented — `rounds` is always an empty array.

**Change:** After each Implementer completion, append the current round record

**Data structure:**

```json
"implementer": {
  "rounds": [
    {
      "round": 1,
      "prompt": "Replace take() with get_or_insert_with",
      "changes": ["crates/desktop/src/commands.rs:987"],
      "reviewer_verdict": "BLOCKER",
      "blockers": ["compilation not verified"]
    },
    {
      "round": 2,
      "prompt": "Fix compilation issues found by Reviewer",
      "changes": ["crates/desktop/src/commands.rs:987"],
      "reviewer_verdict": "PASS",
      "blockers": []
    }
  ]
}
```

**Implementation:**

`write_blackboard_partition` already receives `agent_type` and `SubAgentResult`. When `agent_type == Implementer`:
1. Read existing blackboard `implementer.rounds` array
2. Append current round (extract changes summary from `SubAgentResult` + previous Reviewer verdict)
3. Write back

**Change size:**

| File | Location | Change |
|------|----------|--------|
| `crates/tui/src/tools/subagent/blackboard.rs` | `write_blackboard_partition` | ~30 lines |
| `crates/tui/src/tools/subagent/mod.rs` | `run_subagent_task` call site | Pass Reviewer verdict (optional — can also read previous partition inside blackboard) |

**Acceptance:** Run CRAFT chain → confirm `.deepseek/blackboards/{task_id}.json` has `implementer.rounds` array length ≥ 1 → each round contains `prompt`, `changes`, `reviewer_verdict`

---

## Implementation Order

| Order | Improvement | Rationale |
|:-----:|-------------|-----------|
| 1 | **C1** — Reviewer shell | One-line change, immediate benefit — removes the biggest pain point of "incomplete review" |
| 2 | **C2** — Explorer coverage | Prevents CRAFT chain going off-track at step one — incomplete information source skews everything downstream |
| 3 | **C3** — rounds recording | Traceability is the foundation of closure — P2 auto fix-loop needs to read history |

Total change size: ~65 lines Rust + ~15 lines prompt.

---

## Structural Limitations Out of Scope This Iteration

| # | Limitation | Why not now |
|---|------------|-------------|
| S1 | Dual Judge — Reviewer and main Agent use the same model | Requires independent model instance or rule engine — large architectural change; validate single-model loop first |
| S2 | Sub-agent context soft landing | Requires sub-agent context snapshot + recovery — ~200 lines; lower priority than C1–C3 |
| S3 | Programmatic validation of Explorer output ("did you really read those 5 files?") | Requires validation layer on `SubAgentResult` — doable, but C2 prompt-layer requirements should be validated first |

---

## Verification Method

Validate all three improvements with the same demo task — same tracing-add task as `craft-demo-001`, comparing before/after C1–C3:

| Dimension | V1 (already run) | V2 expected |
|-----------|------------------|-------------|
| Reviewer runs `cargo check` | ❌ Blocked by P3 trimming | ✅ Executes directly |
| Explorer outputs coverage report | ❌ None | ✅ `## Coverage Report` + `files_examined` |
| Blackboard `rounds` | Empty array | Contains each Implementer iteration record |
