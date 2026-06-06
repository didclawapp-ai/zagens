# Weekly report

**Skill:** `office-weekly-report` · **Output:** DOCX

## What it does

Drafts a weekly status report from your notes or attachments — what you completed, what's next, and blockers.

## Before you start

- Task type: **Office**
- Optional: progress notes, tickets, or spreadsheets in `inbox/` or `data/`

## How to run

1. Tap **Weekly report** on the empty state, or ask:
   > Write this week's work report. Confirm date range and audience first.
2. Provide: reporting period, recipient (manager / team), completed work, next-week plan, risks.
3. The agent reads attachments with `read_office` when present, then writes DOCX to `deliverables/`.

## Output sections (typical)

**This week** · **Next week** · **Risks & blockers**

## Verify

- All three sections are present
- Dates and week number in the title match your request

## Tips

- Paste bullet points in chat if you have no files yet.
- Ask for revisions: "add a risk about vendor delay" — the agent can update the same file.

Related: [Executive brief](/docs/office/skills/executive-daily) · [Meeting minutes](/docs/office/skills/meeting-minutes)
