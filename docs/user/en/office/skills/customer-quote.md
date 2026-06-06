# Customer quote

**Skill:** `office-customer-quote` · **Output:** XLSX with tax totals

## What it does

Builds a customer quotation from a price list and requirements in `data/`, computing line items and **tax-inclusive totals** in Excel.

## Before you start

- Task type: **Office**
- Add price list CSV/XLSX and requirements (e.g. `价目表.csv`, `客户需求.md`) under `data/`
- Demo files ship in `docs/harness/fixtures/office-demo/`

## How to run

1. Tap **Customer quote** or ask:
   > Prepare a quote from data/价目表.csv and data/客户需求.md with tax-inclusive totals.
2. Answer questions on currency, tax rate, or discounts if asked.
3. Open the XLSX from `deliverables/`.

## Output (typical)

Quote header · line items · subtotals · tax · grand total · notes

## Verify

- Totals recompute correctly in Excel
- SKU/pricing matches the source price list

**Full P0 walkthrough:** [P0-4 customer quote](/docs/office/p0-quote)

Related: [Office workspace](/docs/office/workspace) · [All skills](/docs/office/skills)
