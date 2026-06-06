# Built-in Office skills

Zagens ships **11 bundled Office skills**. Each maps to a task card on the Office empty state and a `load_skill` playbook in the runtime.

## Skill index

| Skill ID | Card (en) | Output | Guide |
|----------|-----------|--------|-------|
| `office-competitive-analysis` | Competitive analysis | DOCX | [Skill](/docs/office/skills/competitive) · [P0-1](/docs/office/p0-competitive) |
| `office-executive-daily-brief` | Executive daily brief | DOCX | [Skill](/docs/office/skills/executive-daily) · [P0-2](/docs/office/p0-executive) |
| `office-production-daily-report` | Production & quality report | DOCX | [Skill](/docs/office/skills/production-daily) · [P0-3](/docs/office/p0-production) |
| `office-customer-quote` | Customer quote | XLSX | [Skill](/docs/office/skills/customer-quote) · [P0-4](/docs/office/p0-quote) |
| `office-weekly-report` | Weekly report | DOCX | [Weekly report](/docs/office/skills/weekly-report) |
| `office-meeting-minutes` | Meeting minutes | DOCX | [Meeting minutes](/docs/office/skills/meeting-minutes) |
| `office-project-report` | Project report PPT | PPTX | [Project report](/docs/office/skills/project-report) |
| `office-data-report` | Data report | XLSX | [Data report](/docs/office/skills/data-report) |
| `office-contract-draft` | Contract draft | DOCX | [Contract draft](/docs/office/skills/contract-draft) |
| `office-resume` | Resume / cover letter | DOCX | [Resume](/docs/office/skills/resume) |
| `office-release-notes` | Release notes | DOCX | [Release notes](/docs/office/skills/release-notes) |

## How skills run

1. **Task type** must be **Office**.
2. Prepare [office workspace](/docs/office/workspace) folders (`inbox/`, `data/`).
3. Tap a card or describe the outcome ("write meeting minutes for today's sync").
4. Agent calls `load_skill` then `read_office` / `web_search` / `write_office` as needed.
5. Deliverables appear under `deliverables/`.

## Custom skills

Add your own under `~/.zagens/skills/` — see [Skills management](/docs/settings/skills).

## Boundaries

- Skills produce **files**, not chat-only summaries (unless you ask for a brief first).
- Code refactors and shell automation → **Code** mode + [LHT](/docs/code/lht).
