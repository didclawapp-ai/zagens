# Zagens desktop — scheduled tasks (automations)

> **Status (2026-06):** Landed in Zagens desktop + runtime sidecar. This document is the **product and implementation reference** for recurring background tasks.

## Overview

Scheduled tasks are **local-first automation rules** that enqueue standard background **Tasks** on an RRULE schedule. Each fired run creates a durable task (same pipeline as `POST /v1/tasks` or the desktop Tasks panel), with its own thread, turn, and run history.

| Surface | Location |
|---------|----------|
| Desktop UI | Settings → **Scheduled tasks** (`ScheduledAutomationsPanel.tsx`) |
| Runtime HTTP API | `/v1/automations` — see [API_DESIGN.md §3.3.7](../tech/API_DESIGN.md) |
| Agent tools | `automation_*` tool family (create/list/run/pause/…) |
| Scheduler core | [`automation_manager.rs`](../../crates/runtime-server/src/automation_manager.rs) |
| RRULE UI helpers | [`scheduleRrule.ts`](../../crates/desktop/web-ui/src/lib/scheduleRrule.ts) |

## Trigger modes

Each automation stores a `trigger_kind`:

| Kind | UI label | Behavior |
|------|----------|----------|
| `prompt` | Lightweight prompt | Enqueues a background task with **conservative defaults**: Agent mode, no shell, auto-approve tools. Model/mode/shell flags on the record are ignored at enqueue time. |
| `task` | Full Task | Enqueues using stored **model**, **mode**, **workspace**, **allow_shell**, **trust_mode**, and **auto_approve** — equivalent to manually creating a Task with those options. |

Use **prompt** for low-risk periodic summaries or scans. Use **task** when the scheduled job needs shell access, a specific model, YOLO mode, or other Task parameters.

## Schedule (RRULE)

The scheduler uses a **subset of iCalendar RRULE** (uppercase, semicolon-separated). Supported `FREQ` values:

| FREQ | Meaning | Common fields |
|------|---------|---------------|
| `MINUTELY` | Every N minutes | `INTERVAL`, optional `BYDAY` (weekday filter) |
| `HOURLY` | Every N hours | `INTERVAL`, optional `BYDAY` |
| `DAILY` | Every N days at a wall time | `INTERVAL`, `BYHOUR`, `BYMINUTE` |
| `WEEKLY` | Selected weekdays at a wall time | `BYDAY` (`MO`…`SU`), `BYHOUR`, `BYMINUTE` |
| `MONTHLY` | Every N months on a day/time | `INTERVAL`, `BYMONTHDAY`, `BYHOUR`, `BYMINUTE` |
| `ONCE` | Single fire | `DTSTART=YYYY-MM-DDTHH:MM:SS` (must be in the future at create time) |

The desktop form builds these strings via `buildRrule()`. **Custom RRULE** accepts any string the runtime parser supports; invalid rules fail at create/update with a clear error.

### Examples

```text
FREQ=DAILY;BYHOUR=9;BYMINUTE=0
FREQ=WEEKLY;BYDAY=MO,WE,FR;BYHOUR=8;BYMINUTE=30
FREQ=MINUTELY;INTERVAL=15;BYDAY=MO,TU,WE,TH,FR
FREQ=HOURLY;INTERVAL=2
FREQ=MONTHLY;INTERVAL=1;BYMONTHDAY=1;BYHOUR=9;BYMINUTE=0
FREQ=ONCE;DTSTART=2030-06-10T09:00:00
```

## Lifecycle

```text
Scheduler tick (sidecar)
  → due automation? → create AutomationRunRecord (queued)
  → task_manager.add_task(...) → Running (task_id, thread_id, turn_id)
  → reconcile_run_statuses() maps task terminal state → Completed / Failed / Canceled
  → advance next_run_at (ONCE schedules auto-pause after fire)
```

Operations available in UI and API:

| Action | Effect |
|--------|--------|
| **Run now** | Enqueue immediately (does not skip the next scheduled slot) |
| **Pause** | Stops future fires; `next_run_at` cleared |
| **Resume** | Recomputes `next_run_at` from RRULE |
| **Edit** | Updates name, prompt, schedule, trigger kind, Task options |
| **Delete** | Removes automation definition (run history under its id remains until manually cleaned) |

**Run history** lists recent runs with status, timestamps, and linked `task_id`. The desktop panel can jump to the Tasks panel and highlight the task when a run completed with a task id.

## Persistence

| Data | Path |
|------|------|
| Automation definitions | `~/.zagens/automations/{id}.json` |
| Run records | `~/.zagens/automations/runs/{automation_id}/{run_id}.json` |

Override directory with `ZAGENS_AUTOMATIONS_DIR` or `DEEPSEEK_AUTOMATIONS_DIR`.

Schema versions: automation records `schema_version: 2`, run records `schema_version: 1`.

## API quick reference

Base path: `/v1/automations` (Bearer auth, same as other runtime routes).

```http
GET    /v1/automations
POST   /v1/automations
GET    /v1/automations/{id}
PATCH  /v1/automations/{id}
DELETE /v1/automations/{id}
POST   /v1/automations/{id}/run
POST   /v1/automations/{id}/pause
POST   /v1/automations/{id}/resume
GET    /v1/automations/{id}/runs
```

Create body (minimal):

```json
{
  "name": "Daily digest",
  "prompt": "Summarize yesterday's git commits in this repo.",
  "rrule": "FREQ=DAILY;BYHOUR=9;BYMINUTE=0",
  "cwds": ["/path/to/workspace"],
  "trigger_kind": "prompt"
}
```

Full Task trigger:

```json
{
  "name": "Nightly audit",
  "prompt": "Run cargo test and file a short report.",
  "rrule": "FREQ=DAILY;BYHOUR=2;BYMINUTE=0",
  "cwds": ["/path/to/workspace"],
  "trigger_kind": "task",
  "model": "deepseek-v4-pro",
  "mode": "agent",
  "allow_shell": true,
  "trust_mode": false,
  "auto_approve": true
}
```

OpenAPI source of truth: [`zagens-runtime-v1.openapi.json`](../tech/openapi/zagens-runtime-v1.openapi.json).

## Desktop UX notes

- Panel lives under **Settings → Scheduled tasks** in the right sidebar (`RightPanel.tsx`).
- Requires a **connected runtime** (local sidecar). The panel shows a waiting state until the API is reachable.
- Schedule editor supports minutely / hourly / daily / weekly / monthly / once / custom RRULE, weekday presets (workdays), and optional weekday restriction on minute/hourly rules.
- Saving automations does **not** restart the sidecar (unlike global Hooks settings).

## Security and operations

- Scheduled tasks inherit the **same sandbox and approval semantics** as manually created Tasks for the chosen `trigger_kind`.
- **Prompt** mode deliberately avoids shell unless you switch to **task** mode and explicitly enable `allow_shell`.
- Automations are **local-only**; there is no cloud scheduler. The sidecar must be running for ticks to fire.
- Idempotency: if a run record already exists for a schedule slot, the scheduler skips duplicate enqueue and still advances `next_run_at`.

## Related docs

- [API_DESIGN.md §3.3.7 Automations](../tech/API_DESIGN.md)
- [TOOLS_PRINCIPLES.md § Tasks & automation](../tech/TOOLS_PRINCIPLES.md)
- [RUNTIME_ARCHITECTURE.md](../tech/RUNTIME_ARCHITECTURE.md)
