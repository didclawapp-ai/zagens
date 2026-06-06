# P0-3: Production & quality morning report

**Skill:** `office-production-daily-report` · **Output:** brief first, then DOCX (optional XLSX)

## What it does

Turns production and quality exports into a morning brief for ops leads — structured metrics, anomalies, and follow-ups.

## Before you start

- Task type: **Office**
- Place yesterday's MES or Excel exports in **`data/`** (e.g. `data/生产日报_昨日.xlsx`)
- Demo fixtures include sample production/quality spreadsheets

## How to run

1. Tap **Production & quality report** on the Office empty state, or say:
   > Report yesterday's production and quality status. Brief first, then DOCX.
2. Read the short text summary the agent posts first.
3. Confirm to generate the full DOCX in `deliverables/`.

## Output sections (typical)

Overview · production metrics · quality metrics · exceptions & risks · items to confirm

## Verify

- KPIs trace back to spreadsheets in `data/`
- **OEE / yield** and other key metrics match the source
- Text brief and confirmation happened before DOCX render

Related: [All P0 demos](/docs/office/scenarios)
