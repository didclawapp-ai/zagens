# Production & quality report

**Skill:** `office-production-daily-report` · **Output:** DOCX morning brief

## What it does

Reads production and quality data from `data/`, summarizes yesterday's status, and produces a morning report DOCX for shop-floor or management review.

## Before you start

- Task type: **Office**
- Place spreadsheets in `data/` (e.g. `生产日报_昨日.xlsx` in demo fixtures)
- `inbox/` is not the standard ingest path; put comparison files under `data/` or state paths in chat

## How to run

1. Tap **Production & quality report** or ask:
   > Report yesterday's production and quality status as a DOCX brief.
2. Review the **text brief** first when the skill asks for confirmation.
3. Confirm to generate the formal DOCX in `deliverables/`.

## Output sections (typical)

Overview · production metrics · quality metrics · exceptions & risks · items to confirm

## Verify

- KPIs match spreadsheets in `data/`
- **OEE / yield** and other key metrics are present
- Exception items are not invented

**Full P0 walkthrough:** [P0-3 production / quality morning report](/docs/office/p0-production)

Related: [Data report skill](/docs/office/skills/data-report) · [Office I/O](/docs/tools/office-io)
