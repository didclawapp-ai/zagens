# Executive daily brief

**Skill:** `office-executive-daily-brief` · **Output:** DOCX with open decisions

## What it does

Aggregates department briefs from `inbox/` into one leadership summary with risks and **pending decisions**.

## Before you start

- Task type: **Office**
- Copy briefs into `inbox/` (DOCX, XLSX, PDF, Markdown)
- Demo fixtures: `docs/harness/fixtures/office-demo/` or [Use cases](/use-cases) zip

## How to run

1. Ensure `inbox/` has yesterday's department files.
2. Tap **Executive daily brief** or ask:
   > Summarize yesterday's inbox briefs for leadership. List open decisions.
3. Review the **text overview** first — the skill **posts a short summary, then asks whether to render the formal DOCX** (`confirm_before_render`).
4. Confirm to write the final document to `deliverables/`.

## Output sections (typical)

Overview · department highlights · risks · **pending decisions** · appendix

## Verify

- Numbers match source attachments
- "Pending decisions" section is present

**Full P0 walkthrough:** [P0-2 executive daily brief](/docs/office/p0-executive)

Related: [Office workspace](/docs/office/workspace) · [All skills](/docs/office/skills)
