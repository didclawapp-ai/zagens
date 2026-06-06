# Skills management

**Skills** are reusable playbooks (`SKILL.md`) the agent loads with `load_skill` — office workflows, audits, custom procedures.

## Built-in Office skills

Zagens ships bundled skills under the runtime assets (e.g. `office-weekly-report`, `office-customer-quote`). Office empty-state cards map to these skills.

See the [built-in skills index](/docs/office/skills) and [P0 demos](/docs/office/scenarios).

## User skills directory

Custom skills live in `~/.zagens/skills/<name>/SKILL.md`.

## Desktop UI

- Sidebar **Tasks** — background tasks; create / import / install skills
- **Settings → Skills** — same skills panel (`AutomationPanel`)

**Note:** Scheduled automation (`GET /v1/automations`) API remains, but the UI **does not list** automations yet.

## Authoring tips

- Keep steps numbered and explicit about inputs (`inbox/`, `data/`).
- Declare expected output format (DOCX, XLSX) and `deliverables/` path.
- Test with a small fixture workspace before production data.

## skill-creator

The bundled `skill-creator` skill helps the agent draft new skills interactively (Code or Office).

Related: [MCP](/docs/settings/mcp) · [Skills index](/docs/office/skills) · [Office workspace](/docs/office/workspace)
