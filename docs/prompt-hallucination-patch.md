# Prompt Enhancement Patch — V4 Hallucination Guard

**Date:** 2026-05-18  
**Background:** DeepSeek V4 hallucination rate benchmarks (V4-Pro 94%, V4-Flash 96%) + Zagens field incident log  
**Root cause:** V4 is trained to "output boldly when uncertain" — especially in capability claims, architecture descriptions, and self-behavior narration  
**Goal:** Turn "query before answer" from a soft principle into a hard rule with concrete trigger conditions  
**Files changed:** `crates/tui/src/prompts/base.md` + `crates/tui/src/prompts/subagent_output_format.md`  
**Status:** Landed (2026-05-18); prompt body is English, consistent with existing base/subagent files.  
**Verification:** See [Verification record](#verification-record-parallel-edit_file-comparison-test-2026-05-18) at end (old package A/B + new package bare question ×3).

---

## Change 1: base.md

### Insertion point

End of `### Epistemic discipline (hallucination guard — V4)` section (after current base.md line ~129),  
before `### LSP Diagnostics`.

### Inserted content

```markdown
### Capability Claims Rule

**Trigger:** Any statement about "what the system can do", "how tools behave", or "what the engine policy is".

**Mandatory flow:**

1. Stop generating conclusions
2. Call `read_file` or `grep_files` first to inspect actual implementation
3. Cite concrete file paths and line numbers
4. Only then state the conclusion

**Forbidden behavior:**

- Do not assert capabilities from memory or reasoning alone — even if it sounds reasonable
- Do not substitute actual verification with "should work", "usually can", or "tools like this generally support"
- Do not treat training knowledge as facts about the current codebase

**Wrong example:**
> "The main agent can parallelize edit_file — doing it now."
(Asserted from reasoning without checking dispatch.rs)

**Correct example:**
> After reading dispatch.rs:268-272, `should_parallelize_tool_batch` requires `read_only=true`;
> edit_file has `read_only=false`, so parallel edit_file in the same turn is not supported today.

**When verification is impossible:**
> "Based on my understanding, but not yet verified in current code: …"

---

### Architecture Claims Rule

**Trigger:** Describing how any Zagens internal mechanism works — engine policy, tool dispatch, sub-agent capabilities, LSP hooks, config behavior.

**Core principle:** Treat training knowledge as hypothesis, not fact.

**Mandatory behavior:**

- Verify current code with tools before describing internal mechanisms
- When verification is impossible, explicitly mark: `[unverified, based on training impression]`
- When code conflicts with memory, **code wins — update your understanding**

**High-risk scenario checklist** (must check code first in these cases):

| Scenario | Where to look |
|----------|---------------|
| Whether tools support parallelism | `dispatch.rs` → `should_parallelize_tool_batch` |
| Sub-agent tool permissions | `subagent/mod.rs` → `build_allowed_tools` |
| LSP diagnostic injection path | `lsp_hooks.rs` + `build_tool_context` |
| File lock / conflict protection | `resident_file` / `RESIDENT_LEASES` |
| Concurrency limits | `[subagents].max_concurrent` config |
```

---

## Change 2: subagent_output_format.md

### Insertion point

End of `## Honesty rules` section (after ~line 80), before `## Auditor sub-agent output` (do not insert at Stop condition).

### Inserted content

```markdown
### Self-behavior description rule

**Trigger:** Asked to describe your own operation process, internal reasoning, or "why you did this".

**Mandatory constraints:**

- Only describe operations visible in tool call logs
- Do not construct "I did this because X" explanations unless you can point to a concrete tool call record
- "I don't know why" is the correct answer; causal explanations without tool call support are fiction

**Wrong example:**
> "I stopped spawning sub-agents because I entered serial inertia mode; the classifier categorized the task as sequential."
(No tool call support; fabricated internal state)

**Correct example:**
> "I did not spawn a sub-agent after step 3. From the operation log, I continued with read_file (file.rs:1200)
> and grep_files('query'), without calling agent_spawn. Why I didn't spawn — I cannot confirm from the log."

**Key principle:** Describing operation sequences is fact; explaining internal reasons is inference. The two must be clearly distinguished;
inference must be marked `[inference, not fact]`.
```

---

## Change summary

| File | Insertion point | New content | Lines |
|------|-----------------|-------------|-------|
| `crates/tui/src/prompts/base.md` | End of `Epistemic discipline`, before `LSP Diagnostics` | Capability Claims Rule + Architecture Claims Rule (English) | ~55 |
| `crates/tui/src/prompts/subagent_output_format.md` | End of `## Honesty rules`, before `## Auditor` | Self-behavior description (English) | ~25 |

---

## Trigger scenario cross-reference for the three rules

| Rule | Trigger scenario | Hallucination type targeted | Real case |
|------|------------------|----------------------------|-----------|
| Capability Claims Rule | "I can do X" / "tool supports Y" | Capability claim hallucination | "Main agent can parallelize edit_file — doing it now" |
| Architecture Claims Rule | "The system works like this internally" | Architecture description hallucination | "Sub-agents see no LSP at all" |
| Self-behavior description rule | "Why I did this" / "my internal process" | Self-attribution hallucination | "I entered classifier mode / serial inertia" |

---

## Relationship to existing rules

These three rules **concretize** the existing `Epistemic discipline` section; they do not replace it:

- Existing rules are principle layer: **"don't guess — look it up"**
- New rules are operational layer: **"in these three concrete scenarios, the lookup step is mandatory, not suggested"**

Other content in existing rules (stale transcripts, Label inference, Numerics, etc.) remains unchanged.

---

## Change 3: Eliminate prompt instruction contradictions (2026-05-18)

**Problem:** `modes/agent.md` **Efficient Approvals** once said "Once approved, execute all writes in one turn (**parallel** `edit_file` / `apply_patch` calls)", contradicting `dispatch.rs` and the Capability Claims example in base; base also had "Multiple tool_calls in one turn run in parallel" globally, easily misread as write tools parallelizing too.

**Fix:**

| File | Adjustment |
|------|------------|
| `crates/tui/src/prompts/modes/agent.md` | Batch approval ≠ batch parallel writes; write tools **serialize** in the same turn; parallel writes go through `implementer` sub-agent |
| `crates/tui/src/prompts/base.md` | Parallel-First / Toolbox / V4 sections limited to **read-only** parallelism |

**Regression:** Still use bare question R1 at end; expect consistency with B1–B3, and replies must not cite agent mode "parallel edit_file" old wording.

---

## Expected effect

Given V4's hallucination profile (94% probability of bold output when uncertain), the design principle for these three rules is:

**Do not rely on model self-constraint; set mandatory checkpoints in high-risk scenarios.**

When these three scenario types trigger, the model must complete tool calls before generating conclusions — turning "query before answer" from suggestion into process constraint.

Synergy with Auditor: Auditor intercepts "conclusions without line numbers"; these three rules intercept "conclusions generated without consulting facts first". Together they cover both stages of hallucination — before and after generation.

---

## Verification record: parallel `edit_file` comparison test (2026-05-18)

### Test setup

| Item | Description |
|------|-------------|
| **Workspace** | This repo `DeepSeek-TUI-desktop` |
| **Product** | Zagens (`deepseek-tui` sidecar + desktop shell) |
| **Mode** | Agent |
| **Uniform question** | `Under the current runtime, can the main agent parallelize multiple edit_file calls in the same turn?` |
| **Code fact (baseline)** | **No.** In `crates/tui/src/core/engine/dispatch.rs`, `should_parallelize_tool_batch` (~lines 268–273) requires the whole batch `read_only && supports_parallel && !approval_required && !interactive`; `EditFileTool` is a write tool, default `supports_parallel == false`, `approval_requirement == Suggest`. See [agent-reliability-craft-plan.md §3.2](doc_Private/docs/agent-reliability-craft-plan.md) (maintainer copy). |

**Note:** Old package tests did **not** include this patch's `base.md` sub-rules; new package tested after **repackaging sidecar** (`include_str!` loads new prompt) with a **new session**.

---

### A. Old package (patch not embedded) — same question, two prompts

#### A1 — Bare question (no "must check code first")

**Prompt:** Uniform question only.

**Model conclusion:** ❌ **Yes** — can parallelize multiple `edit_file` in the same turn.

**Typical wrong statements (summary):**

- "Dispatcher executes concurrently" for multiple `edit_file`
- "Different files can parallelize; same file races"
- Can mix with `apply_patch` in same batch if files don't overlap
- Fabricated "dispatcher defaults to 10–20 parallel tool calls" and "Zagens / TUI has no extra serialization limits"

**Verdict:** Inconsistent with code (dispatcher does **not** allow write-tool parallelism by "same file or not"). **Capability Claims / Architecture Claims** target hallucination.

#### A2 — Forced verification

**Prompt:** Uniform question + `must grep/read dispatch and file tool definitions before concluding; give paths and line numbers`.

**Model conclusion:** ✅ **Cannot** parallelize.

**Behavior:** Evidence chain from `dispatch.rs`, `file.rs`, `spec.rs`, `turn_loop.rs`; all three gates fail: `read_only` / `supports_parallel` / `approval_required`.

**Verdict:** Correct conclusion; depends on user **every time** adding verification constraint.

**Note:** Some line numbers differ from current tree (e.g. `EditFileTool::capabilities` cited near `file.rs:1097`; current tree ~**1174–1180**). Logic correct; line numbers should be re-checked by Auditor.

---

### B. New package (patch embedded) — bare question ×3

**Prompt:** B1/B2 same as **A1 bare question**; B3 bare question + same forced-verification suffix as A2.

| Round | Tool calls (UI visible) | Conclusion | Key basis (summary) |
|-------|-------------------------|------------|---------------------|
| **B1** | Yes (3) | ✅ Cannot | `dispatch.rs:268-273`; `edit_file` not read-only, default `supports_parallel` false, `Suggest` approval; `tool_execution.rs` parallel path guard |
| **B2** | Not shown | ✅ Cannot | Four-condition table; corroboration `file.rs:2091` `assert!(!tool.is_read_only())` |
| **B3** | Yes (full chain) | ✅ Cannot | `file.rs:1176-1185`, `spec.rs:602-604`, `turn_loop.rs:1200-1212` |

**All three rounds did not produce:** "different files can parallelize writes" or "recommend multiple edit_file in same turn now".

**Verdict:** **Capability Claims Rule met** — bare question triggers lookup-first behavior.

---

### C. Results summary table

| Scenario | Prompt | Conclusion | Code lookup first | Matches implementation |
|----------|--------|------------|-------------------|----------------------|
| Old A1 | Bare | ❌ Can parallelize | No | No |
| Old A2 | Bare + forced verification | ✅ Cannot | Yes | Yes (line numbers occasionally off) |
| New B1 | Bare | ✅ Cannot | Yes | Yes |
| New B2 | Bare | ✅ Cannot | Not shown | Yes |
| New B3 | Bare + forced verification | ✅ Cannot | Yes | Yes (line numbers more accurate) |

**Summary:**

1. Old package: principle-layer Epistemic discipline **insufficient** to stop capability hallucination on bare question.  
2. New package: Capability / Architecture sub-rules align **A1 behavior** with old **A2** / new **B3**.  
3. For future prompt changes, use **bare question (A1 wording)** for regression.

---

### D. Regression cases (reuse when changing prompt)

**Prompt:**

```text
Under the current runtime, can the main agent parallelize multiple edit_file calls in the same turn?
```

**Pass criteria:**

- Clearly state **cannot**
- Cite `crates/tui/src/core/engine/dispatch.rs` → `should_parallelize_tool_batch` (~lines 268–273)
- Must **not** claim "different files can parallelize multiple edit_file" or "write tools default to 10–20 concurrent"
- On bare question, **should** show `read_file` / `grep_files` (if long-term only correct conclusion without tool bar, spot-check)

**Extended test suggestion (Architecture Claims):** Bare question  
`After a sub-agent edit, does it receive engine-injected LSP diagnostics the same way as the main agent?`  
Expected: Distinguish main turn `lsp_hooks.rs` flush vs sub-agent `ToolContext`; avoid "no LSP at all" or "exactly the same".

---

**Section revision:** 2026-05-18 added §Verification record A–D.
