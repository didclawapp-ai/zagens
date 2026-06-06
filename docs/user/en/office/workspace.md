# Office workspace

Office mode expects a **document-centric** folder layout instead of a git repo.

## Recommended structure

```
my-office-workspace/
  inbox/          # briefs, notes, exported emails (DOCX, MD, PDF, …)
  data/           # price lists, reference spreadsheets (CSV, XLSX)
  deliverables/   # agent output (created on first write_office)
```

| Folder | Purpose |
|--------|---------|
| `inbox/` | Input the agent reads — department dailies, meeting notes |
| `data/` | Structured tables — price lists, KPI exports |
| `deliverables/` | DOCX / XLSX / PPTX / PDF the agent writes |

`inbox/` and `data/` are **not** auto-created — add them manually or copy fixtures ([Use cases](/use-cases) zip). See [Deliverables](/docs/office/deliverables).

## Preview and handoff

- Click files under `deliverables/` to preview extracted text
- [Open in system apps](/docs/office/deliverables) when you need full formatting
- New outputs are highlighted after `write_office` completes

## What Office mode does not do

- No embedded terminal or repo-wide refactors — use **Code** mode for engineering
- Agents deliver **files**, not pasted chat walls

Try the [P0 demos](/docs/office/scenarios) or jump to a single scenario:

- [Competitive analysis](/docs/office/p0-competitive)
- [Executive daily brief](/docs/office/p0-executive)
- [Production report](/docs/office/p0-production)
- [Customer quote](/docs/office/p0-quote)
