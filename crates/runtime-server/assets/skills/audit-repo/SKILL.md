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

**Phase B tools (required):** `scratchpad_init`, `scratchpad_status`, `scratchpad_append`, `scratchpad_list_notes`, `scratchpad_set_area`, `scratchpad_import_agent`, `scratchpad_verify_note`. Pass `run_id` or rely on `thread_id` / bound `scratchpad_run_id`.

**Order:** import or append ≥1 note → verify HIGH/BLOCKER → `scratchpad_set_area(done)` (runtime rejects `done` with open HIGH/BLOCKER or zero notes).

`write_file` is **fallback only** when scratchpad tools truly fail.

## P0 — Inventory (`inventory.json`)

**Preferred:** auto-generate from workspace members (includes `runtime-server`, desktop web-ui, all `Cargo.toml` members):

```
scratchpad_init({ "template": "workspace_audit", "scope": "…" })
```

Manual rows remain supported via `areas[]`. Granularity: one row = one check-complete unit (~crate first-level subdir), **10–40 rows**, **≤ ~20 source files** per row.

After building inventory, append:

`{"kind":"meta","area_id":"_global","claim":"inventory_version 1, N areas"}`

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

### Join (mandatory — do not hand-copy prose)

1. `agent_list` until no `Running`.
2. For each completed explore agent:
   - `scratchpad_import_agent({ agent_id, area_id? })` — imports `structured_findings` as **`status=open`**, `source=agent:{id}`.
3. For each imported **HIGH/BLOCKER** (and MEDIUM you will report):
   - `read_file` / `grep_files` (caller-trace for security claims).
   - `scratchpad_verify_note({ note_id })` — promotes to **`verified`** (append-only supersede).
4. `scratchpad_set_area({ area_id, status: "done" })` — **blocked** while open HIGH/BLOCKER remain.

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

`todo` / `open` must **not** appear in the final report.

## P2 — Synthesize

1. Every area `done` or `deferred` (with meta reason).
2. Collect `kind=finding` + `status=verified`, not superseded.
3. Draft report from those lines only (cite `note_id` or `file:line`).

## P3 — Verify

`agent_spawn(type=auditor)` for HIGH/BLOCKER per `base.md`. Auditor reads scratchpad note ids (track A) and prose consistency (track B).

## Reference

Design doc: `docs/desktop/audit-scratchpad-design.md`
