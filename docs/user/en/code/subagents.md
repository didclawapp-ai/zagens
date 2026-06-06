# Sub-agents panel

**Sub-agents** are child workers spawned by the main agent via `agent_spawn` / `delegate_to_agent`. CRAFT roles (Explore, Implementer, Review, …) appear here live; the bottom section summarizes **CRAFT blackboard tasks**.

> **Code mode only.** Office has no sub-agents sidebar entry.

---

## How to open

| Entry | Notes |
|-------|-------|
| Sidebar → **Agents** | Full-height tracking |
| **Audit grid** → bottom-right | With checklist, scratchpad, LHT ([UI tour](/docs/ui-tour)) |

Title bar **Show audit grid** when harness data exists (Code).

---

## Layout

### Header stats (three columns)

| Column | Counts |
|--------|--------|
| **Running** | `spawned` \| `running` |
| **Completed** | `completed` |
| **Interrupted** | `interrupted` (failed/cancelled terminals) |

Only rows with ids like **`agent_*`** (not parent `call_*` tool ids).

### Agent cards (expandable)

| Element | Meaning |
|---------|---------|
| Status dot | Running pulse / complete green / interrupted red |
| **Title** | `nickname` or type + badge |
| **agent_id** | Full monospace id |
| **Objective** | First ~200 chars of spawn prompt |
| **progressStatus** | Latest progress line while running (SSE) |
| **Steps done/max** | vs `max_steps` (default cap 100) |
| **Step cap Ns** | From `step_timeout_ms` |
| **Stuck suspected** | No heartbeat past timeout + buffer |
| **Work package task_id** | CRAFT blackboard / scratchpad run |
| Footer | Tool count · tokens · duration |
| Expanded | Full objective, tool calls, `resultSummary` |

Updates via runtime **SSE** and polling.

---

## Agent types and tools

Spawn `type` / `agent_type` ([CRAFT](/docs/code/craft)):

| Type | Aliases | Tool clip |
|------|---------|-----------|
| **explore** | exploration, explorer | **Read-only**: list/read/grep/glob/file_search/web/note |
| **review** | reviewer, code_review | **Read-only + exec_shell** (verify commands) |
| **implementer** | implement, builder | **Inherits parent** |
| **verifier** | verify, tester | **Inherits parent** |
| **auditor** | audit, fact_checker | Audit prompts + tools |
| **plan** | planning | Planning |
| **general** | default, worker | **Inherits parent** |
| **custom** | — | **Requires** explicit `allowed_tools` |

Review uses `exec_shell` for `cargo check`, `npm test`, etc. Implementer may get a **git stash** snapshot before edits (non-fatal rollback).

---

## CRAFT tasks section

When `.zagens/blackboards/*.json` exists, bottom **CRAFT tasks** list (sorted by `task_id`):

| Field | Source |
|-------|--------|
| **task_id** | Blackboard filename |
| **Explorer yes/—** | `explorer` partition present |
| **Implementer rounds** | `implementer.rounds` length |
| **Reviewer** | `reviewer.verdict` — PASS / BLOCKER / MAJOR / FAIL |
| **Verifier** | `verifier.summary` if any |

Quick fix-loop status without opening JSON.

---

## Settings

**Settings → System → Security**:

| Setting | Range | Default |
|---------|-------|---------|
| **Sub-agents enabled** | On/off | On |
| **Max sub-agents** | 1–20 | From config |
| **Step timeout** | 120–1800 s | **600s** |

Spawn **`step_timeout_ms`** overrides one job. Large Explore: 10–30 min typical.

Strict: `[features] subagents = false` in config.

See [execution policy](/docs/settings/exec-policy).

---

## Scratchpad / LHT linkage

- Scratchpad **audit** quadrant shows **N sub-agents active** — matches running count here.
- **Narrative spawn** warning when chat claims spawn but no `agent_*` rows.
- LHT **macro loop** CRAFT Review appears here and under CRAFT tasks.

---

## Tips

1. One **`task_id`** per fix-loop; order **Explore → Implementer → Review → Verifier** ([CRAFT](/docs/code/craft)).
2. On **stuck suspected**, ask parent to cancel or narrow scope.
3. Finish review before new scope in the same macro cycle.
4. Parallel Explore is read-only; concurrent Implementer writes follow runtime ownership rules.

---

Related: [CRAFT](/docs/code/craft) · [Audit scratchpad](/docs/code/audit-scratchpad) · [LHT settings](/docs/settings/lht)
