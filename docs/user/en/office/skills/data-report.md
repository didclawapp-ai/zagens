# Data report

**Skill:** `office-data-report` · **Output:** XLSX

## What it does

Reads CSV/XLSX from `data/`, computes summaries, and produces an Excel report with detail and rollup sheets.

## Before you start

- Task type: **Office**
- Place source tables under `data/` (CSV or XLSX)
- Large files: agent may paginate with `start_row` / `limit` — summarize in chat, not full paste

## How to run

1. Tap **Data report** or ask:
   > Build an Excel report from data/sales_q1.csv. Include detail and summary sheets.
2. Confirm: report theme, key metrics, whether you need a chart sheet.
3. XLSX appears in `deliverables/`.

## Sheets (typical)

**Data detail** · **Summary** · optional chart sheet

## Verify

- Row counts and totals match source data
- Headers and units are labeled clearly

## Tips

- Pair with [production daily](/docs/office/skills/production-daily) when source data is operational metrics.
- Open in Excel to validate formulas on complex models.

Related: [Office I/O](/docs/tools/office-io) · [Office workspace](/docs/office/workspace)
