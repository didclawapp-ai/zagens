# Audit scratchpad

The **audit scratchpad** is structured working memory for long **Code** repo audits: findings, evidence, and per-area progress on disk — across turns and sub-agents — instead of chat context alone.

> **Office mode rarely needs scratchpad** — use DOCX sections instead.

---

## On-disk layout

Each **run** is a directory (default `run_id` = current **thread_id**):

```
{workspace}/.zagens/scratchpad/{run_id}/
  inventory.json    # audit areas + status
  notes.jsonl       # append-only findings / meta
```

(Legacy installs may use `.deepseek/scratchpad/`.)

**Thread binding:** Status is bound to the **thread** — other sessions **do not inherit** a prior run. New thread → **Init** again or `scratchpad_init`.

---

## Where to open it

### Audit grid (recommended)

1. **Code** session with checklist / scratchpad / agent activity → title bar **Show audit grid**.
2. **2×2 layout**: checklist | **audit (scratchpad)** | LHT | agents.
3. Drag left edge to resize; **Hide audit grid** to collapse.

Sidebar **Agents** alone does not show full scratchpad — use the audit quadrant.

### Empty state

Shows:

- No scratchpad bound to this session yet
- Needs `scratchpad_init` or the button below
- Path hint: `{workspace}/.zagens/scratchpad/{thread_id}/`
- **Init scratchpad** — runtime HTTP init, default `run_id` = current thread

---

## Panel fields (matches UI)

After a run is bound:

### Progress line

`Progress accounted/total (pct%)` — **accounted** = `done + in_progress + deferred`.

Plus: `done N` · `in progress N` · `deferred N`.

### Checklist dual-track (latest run)

If LHT checklist exists: `Checklist completed/total`.

Amber **Checklist complete but inventory not accounted** when out of sync — use `scratchpad_set_area`, not checklist alone.

### Resume

**Resume area** `{area_id}` when set — agent should continue there.

**Inventory complete** when all areas accounted and total > 0.

### Findings strip

`verified V · open O`, split by severity: **HIGH**, **MED**, **LOW**, **✓HIGH** (verified high).

### Badges

| Badge | Meaning |
|-------|---------|
| **N sub-agents active** | Running sub-agents (latest run) |
| **Needs scratchpad_set_area (notes without inventory)** | Contract violation |
| **Checklist / inventory mismatch** | Dual-track desync |
| **Narrative spawn / no panel data** | Chat claimed spawn but no `agent_*` rows |

### Inventory list

Collapsible table per area:

| Column | Meaning |
|--------|---------|
| **Status** | `pending` · `in_progress` · `done` · `deferred` |
| **area id** | e.g. `workspace`, `crates-runtime` |
| **path** | Workspace-relative — click to open in workbench |
| **notes count** | Rows in `notes.jsonl` for that area |

### Previous runs

Older runs on the same thread as collapsible **Previous audit** cards.

---

## Runtime tools (agent)

| Tool | Role |
|------|------|
| **`scratchpad_init`** | Create run; `areas[]` or `template=workspace_audit` (Cargo members) |
| **`scratchpad_status`** | Progress, findings, `resume_area_id` (same as panel) |
| **`scratchpad_append`** | Append one `notes.jsonl` line |
| **`scratchpad_set_area`** | Set area → `done` / `deferred` / `in_progress` |
| **`scratchpad_verify_note`** | Mark finding **verified** after read/grep (often required before `done` on HIGH/BLOCKER) |
| **`scratchpad_list_notes`** | List notes for one `area_id` (do not rely on tail of jsonl) |

### Note `kind` (common)

| kind | Use |
|------|-----|
| **finding** | Audit finding (reports use verified only) |
| **cleared** | Resolved issue |
| **meta** | Defer reason, etc. |
| **todo** | Follow-up |

### Area flow (recommended)

```text
pending → in_progress → done
                    ↘ deferred (meta reason)
```

Before **done**: append ≥1 note → verify if needed → **`scratchpad_set_area(done)`**.

Checklist alone does **not** replace inventory accounting.

---

## With CRAFT

| Mechanism | Role |
|-----------|------|
| **Scratchpad** | Long audit **area progress + evidence** (multi-run, resumable) |
| **CRAFT blackboard** | One **`task_id`** Explore/Review **structured handoff** |

Typical **repo audit** ([craft-audit preset](/docs/settings/lht)):

1. LHT harness **off** (or composer **LHT·off**)
2. Init inventory via panel or `scratchpad_init`
3. **Explore** per area (read-only), import/append findings
4. **Implementer** if fixes needed; **Review** → PASS/BLOCKER
5. Report cites **verified** findings only

Explore may bind **`scratchpad_run_id`** for isolation.

---

## Checklist dual-track

LHT uses `checklist_write` for engineering tasks; scratchpad **inventory** tracks **audit coverage**.

When checklist items complete, matching inventory areas should **`scratchpad_set_area(done)`**. Panel warns on `checklist_inventory_mismatch`.

---

## Tips

1. Large repo: `template=workspace_audit` or manual `areas` by crate.
2. Resume: `scratchpad_status` → `scratchpad_list_notes` for **that area only**.
3. Parallel Explore for subtrees — integrate into scratchpad; do not skip files silently.
4. **`Failed: API call timed out`** ≠ area done — respawn smaller scope or raise timeout.
5. Before report: all areas `done|deferred`; contract checks enforced.

### Example prompt

> Init scratchpad (workspace_audit). From resume area, read sources, scratchpad_append findings, verify, scratchpad_set_area(done). When all areas done, write Markdown report with verified findings only.

---

## FAQ

**Panel stuck on empty?**  
Init on **this thread**; other sessions’ runs do not show. Use **Init scratchpad** or `scratchpad_init`.

**100% progress but open findings?**  
Progress is **areas accounted**, not findings cleared.

**Delete a run?**  
No UI delete; manual cleanup under `.zagens/scratchpad/{run_id}/` (careful).

**Office mode?**  
Scratchpad tools not registered — use [Office skills](/docs/office/skills).

---

Related: [CRAFT](/docs/code/craft) · [Sub-agents](/docs/code/subagents) · [LHT settings](/docs/settings/lht) · [Checklist](/docs/code/checklist)
