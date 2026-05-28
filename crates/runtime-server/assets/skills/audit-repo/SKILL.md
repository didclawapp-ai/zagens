---
name: audit-repo
description: Full-repository code audit with structured scratchpad (inventory.json + notes.jsonl). Use for 全库评审, repo-wide audit, exhaustive code review of the whole tree—not single-file or PR-only reviews.
metadata:
  short-description: Full-repo audit scratchpad workflow
---

# Full-repository audit (scratchpad)

Use this skill when the user wants an **exhaustive, code-level review** of a large tree. Pair with `base.md` § Full-repository code review mode (verification, Auditor, caller-trace).

**Do not use** for one module, a single PR, or a quick skim—use the normal lightweight workflow.

## External memory (mandatory)

Reasoning is ephemeral. Durable facts live only under:

`.deepseek/scratchpad/{run_id}/`

| File | Role |
|------|------|
| `inventory.json` | Area inventory (`areas[].id`, `path`, `status`) — machine-readable audit coverage |
| `notes.jsonl` | One JSON object per line (append-only) |

**Not the same as the sidebar 清单:** Zagens **Checklist** panel shows only **`checklist_write`** data. `inventory.json` alone does **not** populate the sidebar.

### `run_id` (pick one)

1. `thread_id` if known (HTTP thread / user message)
2. else `task_id` if using CRAFT `agent_spawn`
3. else UTC folder name `YYYY-MM-DD-HHmmss`

**Phase B tools (required):** `scratchpad_init`, `scratchpad_status`, `scratchpad_append`, `scratchpad_list_notes`, `scratchpad_set_area`, `scratchpad_import_agent`, `scratchpad_verify_note`. Pass `run_id` or rely on `thread_id` / bound `scratchpad_run_id`.

**Sidebar visibility (required):** `checklist_write`, `checklist_update` (and `checklist_list` when you need ids).

After `scratchpad_init` succeeds you may **`agent_spawn` in the same turn** — runtime syncs the bound run and eager-loads sub-agent tools without a separate `tool_search` step.

**Order:** import or append ≥1 note → verify HIGH/BLOCKER → `scratchpad_set_area(done)` (runtime rejects `done` with open HIGH/BLOCKER or zero notes).

`write_file` is **fallback only** when scratchpad tools truly fail.

## P0 — Inventory (`inventory.json`) + sidebar checklist

**Preferred:** auto-generate from workspace members (includes `runtime-server`, desktop web-ui, all `Cargo.toml` members):

```
scratchpad_init({ "template": "workspace_audit", "scope": "…" })
```

Manual rows remain supported via `areas[]`. Granularity: one row = one check-complete unit (~crate first-level subdir), **10–40 rows**, **≤ ~20 source files** per row.

After building inventory, append:

`{"kind":"meta","area_id":"_global","claim":"inventory_version 1, N areas"}`

### Sidebar checklist (mandatory — same turn as init)

Immediately after `scratchpad_init`, call **`checklist_write`** with **one todo per inventory area**:

```
checklist_write({
  "todos": [
    { "content": "area-core: crates/core", "status": "pending" },
    { "content": "area-runtime-server: crates/runtime-server", "status": "pending" }
  ]
})
```

Rules:

- **`content`** — `{area_id}: {path}` so rows stay aligned with `inventory.json` (checklist ids are auto-assigned integers).
- **Exactly one `in_progress`** at a time (mark the area you are examining now).
- **Keep dual tracks aligned** whenever inventory status changes:

| `inventory.json` | `checklist_write` / `checklist_update` |
|------------------|----------------------------------------|
| `pending` | `pending` |
| `in_progress` | `in_progress` |
| `done` | `completed` |
| `deferred` | `completed` (append reason in `content`, e.g. `area-x: path (deferred: out of scope)`) |

- Prefer **`checklist_update({ id, status })`** when only one area moves; full **`checklist_write`** only when adding/removing rows or reordering.
- After spawn batches, update checklist for areas whose explore agents finished import + `scratchpad_set_area`.

## P1 — Examine (structured sub-agent pipeline)

### Parallel areas

- **Use `agent_spawn(type=explore)`** with `task_id` = `run_id` (blackboard only).
- **`step_timeout_ms` by inventory file count** (count source files under the area path before spawn):

| Area source files | `step_timeout_ms` |
|-------------------|-------------------|
| ≤10 | 600000 |
| 11–20 | 900000 |
| 21–40 | 1200000 |
| >40, or path is `crates/runtime-server/src/tools` | 1800000 |

- Assignment MUST include the inventory **`area_id`** and **`path`** so the child emits matching `<!-- audit-findings -->`.
- **Copy exact `areas[].id` from inventory** — do not invent IDs like `area-core-engine-part1` when inventory says `area-core-part1`. Wrong IDs block import unless you pass `scratchpad_import_agent({ area_id: "<inventory id>" })` or `area_path` matches inventory.
- Before spawning an area batch, mark those areas **`in_progress`** in both scratchpad (if needed) and **`checklist_update`**.

### Join (mandatory — do not hand-copy prose)

1. `agent_wait` / `agent_result(block)` — **omit `timeout_ms`** to use adaptive wait (`step_timeout_ms × remaining steps`, clamped). Explicit `timeout_ms` still overrides when you need a hard cap. On **`timed_out`**, **`agent_cancel`** stuck agents and import completed ones — do not block the parent turn with prose-only steps while agents are Running.
2. Poll with `agent_list`; import each **Completed + NaturalBreak** agent — do **not** require zero Running before P2 when closing out a partial audit.
3. **Outlier join (mandatory when batch >1):** After the first `agent_wait` or when most agents have finished (~200s typical for explore), call `agent_list`. Compute median `duration_ms` among **Completed** agents in this batch (`M`). For each agent still **`Running`** with `duration_ms > max(2×M, 300000)` **or** `stuck_suspected: true` on the snapshot: **`agent_cancel`** → **`scratchpad_import_agent`** if partial output exists → **`scratchpad_append({ kind: "meta", area_id, claim: "deferred: sub-agent outlier (<reason>)" })`** + **`scratchpad_set_area({ area_id, status: "deferred" })`**. Do not wait for NaturalBreak on outliers — proceed with partial close-out.
4. For each completed explore agent:
   - Check sentinel / `agent_result` **`completion_reason`**: only treat as fully done when **`NaturalBreak`**. **`StepLimitReached`** means the child hit the step cap — re-spawn a narrower scope or raise limits; **do not** `scratchpad_set_area(done)` on step-limit alone.
   - `scratchpad_import_agent({ agent_id, area_id? })` — imports `structured_findings` as **`status=open`**, `source=agent:{id}`. Pass **`area_id`** when the child used a wrong id; runtime also remaps by **`area_path`** when it matches an inventory row.
5. For each imported **HIGH/BLOCKER** (and MEDIUM you will report):
   - `read_file` / `grep_files` (caller-trace for security claims).
   - `scratchpad_verify_note({ note_id })` — promotes to **`verified`** (append-only supersede).
6. `scratchpad_set_area({ area_id, status: "done" })` — **blocked** while open HIGH/BLOCKER remain; prefer **`completion_reason=NaturalBreak`** plus imported findings (or explicit “no findings” in prose).
7. **`checklist_update`** the matching row to **`completed`** (same turn as step 6).

**Do not use `task_create`** for per-area audits during bound scratchpad (E5). Use **`agent_spawn`** only.

### Sub-agent output contract

Explore agents MUST end with:

```
<!-- audit-findings -->
{
  "area_id": "area-runtime-server-tools",
  "area_path": "crates/runtime-server/src/tools",
  "items": [
    {
      "kind": "finding",
      "severity": "HIGH",
      "file": "crates/…/file.rs",
      "line": 120,
      "line_end": 145,
      "claim": "One falsifiable sentence",
      "evidence": "read_file:… / grep_files: callers …"
    }
  ],
  "summary": "…"
}
```

They also emit `<!-- craft-verdict -->` for CRAFT blackboard compatibility (runtime maps audit-findings → verdict when needed).

### `notes.jsonl` line schema

| Field | Rule |
|-------|------|
| `area_id` | Must match `inventory.json` `areas[].id` |
| `kind` | `finding` \| `todo` \| `cleared` \| `meta` |
| `status` | `open` \| `verified` \| `deferred` — **report uses verified only** |
| HIGH/BLOCKER | `file` + `line` required; prefer `line_end` |
| `source` | `agent:{id}` until parent verifies; then superseding `verified` row with `source=main` |
| `supersedes` | Append-only severity upgrade or verification of prior `note_id` |

**C1 quality gate (P2 write_file):**

| Inventory status | Counts toward `accounted_ratio` when |
|------------------|--------------------------------------|
| `done` | ≥1 `kind=finding` **or** `kind=cleared` ( **`meta` alone does not count** ) |
| `deferred` | ≥1 `kind=meta` with non-empty `claim` (defer reason) |

When an area has **no actionable findings**, append `kind=cleared` (e.g. `"claim":"no findings in scope"`) **before** `scratchpad_set_area(done)`. Do **not** use only a `meta` summary for `done` — that leaves the area out of the 60% hard gate and blocks report writes.

`todo` / `open` must **not** appear in the final report.

## P2 — Synthesize

1. Every area **`done`** or **`deferred`** (with meta reason) — required before `write_file` to audit deliverables (`deliverables/*audit*`, `doc/*audit*`, `CODE_AUDIT*.md`).
2. Collect `kind=finding` + `status=verified`, not superseded.
3. Draft report from those lines only (cite `note_id` or `file:line`).
4. **Same turn:** after inventory closes, **`write_file` the report in the same turn** — do not end with prose-only summary. Runtime injects a continue nudge when scratchpad is active but P2 gates fail.

**Runtime dual-track warnings:** When checklist shows completed rows but inventory has no matching `done`/`deferred`, `checklist_write` / `checklist_update` append `WARNING checklist_inventory_mismatch`. `scratchpad_status` includes `contract_hints` — treat as blocking guidance.

### Partial audit / closing out (avoid deadlocks)

When you cannot finish all areas in one session:

1. Import all completed agents (use `area_id` override or path remap if needed).
2. For each remaining **`pending`** area: `scratchpad_append({ kind: "meta", area_id, claim: "deferred: <reason>" })` then `scratchpad_set_area({ area_id, status: "deferred" })`. **`deferred` defaults `require_min_notes=0`** but still needs the **meta** note when `require_deferred_meta` is on.
3. Only then `write_file` the report; state **coverage gaps** explicitly in the report body.

**Do not** end a turn with prose-only output while sub-agents are still **Running** — the parent turn waits for completions.

**Do not** end a turn with prose-only output when scratchpad inventory is still open — runtime will inject a P2 continue message; call `scratchpad_status`, close areas, then `write_file`.

## P3 — Verify

`agent_spawn(type=auditor)` for HIGH/BLOCKER per `base.md`. Auditor reads scratchpad note ids (track A) and prose consistency (track B).

## Reference

Design doc: `docs/desktop/audit-scratchpad-design.md`
