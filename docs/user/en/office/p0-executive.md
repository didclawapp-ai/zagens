# P0-2: Executive daily brief

**Skill:** `office-executive-daily-brief` · **Output:** DOCX with open decisions

## What it does

Aggregates multiple department briefs from `inbox/` into one executive summary with risks and **pending decisions**.

## Before you start

- Task type: **Office**
- Copy sample briefs into `inbox/` (`docs/harness/fixtures/office-demo/` or [Use cases](/use-cases) zip)
- Formats: DOCX, XLSX, PDF, Markdown

## How to run

1. Ensure `inbox/` contains yesterday's department files.
2. Tap **Executive daily brief** on the empty state, or ask:
   > Summarize yesterday's inbox briefs for leadership. List open decisions.
3. Review the **text overview** first — the skill asks before generating the formal DOCX.
4. Confirm to write the final document to `deliverables/`.

## Output sections (typical)

Overview · department highlights · risks · **pending decisions** · appendix

## Verify

- Numbers match source attachments (no invented figures)
- "Pending decisions" section is present

Related: [All P0 demos](/docs/office/scenarios)
