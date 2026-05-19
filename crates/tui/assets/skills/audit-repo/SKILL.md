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

**Phase B tools (required when available):** `scratchpad_status`, `scratchpad_append`, `scratchpad_list_notes`, `scratchpad_set_area`. Pass `run_id` or rely on `thread_id` / bound `scratchpad_run_id`. **Order:** append ≥1 note, then `scratchpad_set_area(done)` (runtime rejects `done` with zero notes). `write_file` is **fallback only**.

## Resume (if folder already exists)

1. `read_file` `inventory.json`.
2. Continue the first `areas[]` with `status` `pending` or `in_progress` (set that row to `in_progress` if needed).
3. Load notes for **that `area_id` only** (`grep_files` on `notes.jsonl` for `"area_id":"<id>"`, or read full file if small)—**not** the tail of the file.
4. Append `kind=meta` with `resumed_at` and `resume_area_id`.
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
