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
| `inventory.json` | Checklist of areas (`areas[].id`, `path`, `status`) |
| `notes.jsonl` | One JSON object per line (append-only) |

### `run_id` (pick one)

1. `thread_id` if known (HTTP thread / user message)
2. else `task_id` if using CRAFT `agent_spawn`
3. else UTC folder name `YYYY-MM-DD-HHmmss`

**Phase B tools (required):** `scratchpad_status`, `scratchpad_append`, `scratchpad_list_notes`, `scratchpad_set_area`. Pass `run_id` or rely on `thread_id` / bound `scratchpad_run_id`. **Order:** append ≥1 note, then `scratchpad_set_area(done)` (runtime rejects `done` with zero notes). `write_file` is **fallback only** when scratchpad tools truly fail.

**Tool discovery:** In Agent mode these four tools are loaded eagerly. If they are missing from the initial tool list (e.g. Plan mode or an older build), call `tool_search_tool_regex` with query `scratchpad` (or `tool_search_tool_bm25` with “audit scratchpad”) **before** falling back to raw file writes.

## Resume (if folder already exists)

1. `scratchpad_status` (preferred) or `read_file` `inventory.json`.
2. Continue the first `areas[]` with `status` `pending` or `in_progress` (set that row to `in_progress` if needed).
3. Load notes for **that `area_id` only** (`grep_files` on `notes.jsonl` for `"area_id":"<id>"`, or read full file if small)—**not** the tail of the file.
4. `scratchpad_append` a `kind=meta` line with `resumed_at` and `resume_area_id` (or append via tool after step 3).
5. Do **not** rebuild inventory unless the user asks to restart.

## P0 — Inventory (`inventory.json`)

```json
{
  "run_id": "2026-05-19-143052",
  "created_at": "2026-05-19T14:30:52Z",
  "areas": [
    {
      "id": "area-tui-engine",
      "path": "crates/tui/src/core/engine",
      "status": "pending",
      "notes": ""
    }
  ]
}
```

**Granularity**

- One row = one **check-complete** unit (typically crate **first-level** subdir, e.g. `crates/tui/src/core/engine`).
- Not whole-repo one row; not one row per `.rs` file.
- Aim for **10–40** rows; **≤ ~20 source files** per row (split if larger).

After building inventory, append to `notes.jsonl`:

`{"kind":"meta","area_id":"_global","claim":"inventory_version 1, N areas"}`

Use `checklist_write` / `update_plan` **in sync** with `inventory.json` statuses.

## P1 — Examine

For each inventory row (in order unless resuming):

1. Set row `status` → `in_progress`.
2. **Batch-read** files under `path` (parallel `read_file` / `grep_files` OK).
3. When **check is complete** for that row (not after every single read), **first** `scratchpad_append` (≥1 line; `area_id` must match inventory), **then** `scratchpad_set_area` with `status=done` (runtime rejects `done` if the area has fewer than `require_min_notes`, default 1).

**Soft rule:** Finish the current area before starting the next. No hard cross-area read-count limit in Phase A.

### P1 — Parallel areas (full-repo only)

**Task (`task_*`) vs Sub-agent (`agent_*`) — do not conflate**

| | **Task** `task_create` | **Sub-agent** `agent_spawn` |
|---|------------------------|----------------------------|
| Relationship | Durable **background work**; peer to the main session (own thread/turn) | **Dispatched by** the parent; parent/child hierarchy |
| Join results | **`task_read`** each completed task | **`agent_result`** / read blackboard after `agent_list` shows terminal |
| P1 parallel area audit | ❌ Do not use to mimic “spawn sub-agents” | ✅ Use `type=explorer` or `worker` |

`task_id` on `agent_spawn` is a **work-package / blackboard key** (often `run_id`), not “you must call `task_create`”.

When multiple inventory rows are examined in parallel:

- **Use `agent_spawn`** with **`task_id` = `run_id`**. Children write **blackboard** only (`.deepseek/blackboards/{run_id}.json`), not scratchpad files.
- **Step timeout:** For each spawn set **`step_timeout_ms`: 240000–360000** (or ensure `[subagents] step_timeout_secs` ≥ 300 in config / Zagens settings). Omitting it uses the config default (often **120 s**), which is too short for multi-file reads — the child **`Failed`** with an API timeout is **not** “area done”; re-spawn with smaller scope or higher timeout before `scratchpad_set_area(done)`.
- **Join before P2:** `agent_list` until no `Running`; each child → **`agent_result`** or blackboard → `scratchpad_append` → `scratchpad_set_area(done)`.
- **Do not use `task_create`** for per-area audits — the engine **blocks** `task_create` while this scratchpad inventory is active (E5). Use **`agent_spawn`** only. For legacy Tasks you already opened, **`task_read` every completed task** before P2; do not call them “sub-agents”.
- **HIGH/BLOCKER from children** are **advisory** until the parent `read_file`/`grep_files` and appends `status=verified`.

If you already used `task_create`: **`task_read` all completed tasks** before P2; never claim they “did not run” without `task_read` evidence.

### `notes.jsonl` line schema

```json
{
  "id": "note-042",
  "area_id": "area-tui-engine",
  "area": "crates/tui/src/core/engine",
  "kind": "finding",
  "severity": "HIGH",
  "file": "crates/tui/src/core/engine/dispatch.rs",
  "line": 120,
  "line_end": 145,
  "claim": "One falsifiable sentence",
  "evidence": "read_file/grep: symbols seen on those lines",
  "status": "verified",
  "supersedes": null
}
```

| Field | Rule |
|-------|------|
| `area_id` | Must match `inventory.json` `areas[].id` |
| `kind` | `finding` \| `todo` \| `cleared` \| `meta` |
| `status` | `open` \| `verified` \| `deferred` — report uses **verified findings only** |
| HIGH/BLOCKER | `file` + `line` required; prefer `line_end` |
| `supersedes` | Severity upgrade: **append one new line** with `supersedes: "note-012"`; **never** rewrite prior JSONL lines; old id is implicitly dropped |

`todo` / `open` are working notes; they must **not** appear in the final report.

## P2 — Synthesize (report draft)

Before writing the report:

1. `read_file` `inventory.json` — every area must be `done` or `deferred` (reason in notes).
2. Collect only `kind=finding` + `status=verified` lines whose `id` is **not** targeted by any other line's `supersedes`.
3. Draft the report from those lines only (cite scratchpad `id` or `file:line`).

Include inventory completeness (areas done vs deferred) and known gaps (timeouts, partial reads).

## P3 — Verify

Follow `base.md`: evidence audit, caller-trace, hype filter, then `agent_spawn(type="auditor")` for HIGH/BLOCKER per existing thresholds.

## CRAFT

- **Explore** agents write **blackboard** only (`.deepseek/blackboards/{task_id}.json`), not scratchpad.
- Parent may use the same `task_id` as `run_id` when both apply; do not duplicate explorer output into notes unless summarizing for the parent.

## Reference

Design doc (maintainers): `docs/desktop/audit-scratchpad-design.md`
