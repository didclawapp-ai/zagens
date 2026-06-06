# P0-4: Customer quote

**Skill:** `office-customer-quote` · **Output:** XLSX with tax totals

## What it does

Builds a customer quote spreadsheet from a **price list** plus **requirements** — line items, quantities, tax, and totals.

## Before you start

- Task type: **Office**
- Put `价目表.csv` or price list XLSX in `data/`
- Put customer requirements in `data/` or `inbox/` (demo: `客户需求.md`)

## How to run

1. Copy `data/` from `docs/harness/fixtures/office-demo/` into your workspace.
2. Tap **Customer quote** on the empty state, or ask:
   > Create a quote XLSX from the price list and customer requirements. Include tax totals.
3. Open the XLSX under `deliverables/` and verify line math in Excel.

## Output (typical)

Sheet with items, unit prices, quantities, subtotal, tax, **grand total**

## Verify

- Prices come from the uploaded list — not invented SKUs
- Tax and total formulas are consistent

Related: [All P0 demos](/docs/office/scenarios) · [Office workspace](/docs/office/workspace)
