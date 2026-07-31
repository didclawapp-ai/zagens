# Zagens Office Scenario Map

> **2026-07-31 (breaking):** Built-in Office mode and `write_office` / `read_office` were **removed** from open-source Zagens. Use skill **`zagens-office`** + external CLI ([install](https://raw.githubusercontent.com/didclawapp-ai/zagens-office/main/install.md)). Rich in-process Office tooling remains a **Zagens Pro** product surface. The catalog below is historical product research.

> **Status:** Product memo (2026-06-05, landing criteria synced 2026-06-05) — **historical** after 2026-07-31  
> **Phase A completion:** L1/L2/L3 foundation landed; L4 has **11** bundled skills + **11** empty-state cards + P0 fixtures/oracle; remaining focus is read-table fidelity stability and P0 end-to-end green runs.  
> **Out of scope this phase (not counted as gaps):** STT/TTS voice (Phase C), ERP/CRM connectors (Phase B), `inbox/`/`data/` workspace auto-initialization (users create or copy from fixtures).  
> **Positioning:** Beyond coding harnesses like LHT / CRAFT, map **Office mode** real work scenarios, alignment with current capabilities, and run-through priorities.  
> **Core thesis (new this version):** These 40+ scenarios are not 40+ independent features, but **the same pipeline × four orthogonal dimensions** with different value combinations. Unified architecture see §2.3 / §3; new scenarios should reduce to "fill one skill contract", not "write a new engine".  
> **Related:** [task-type-prompt-architecture.md](../task-type-prompt-architecture.md), [COMPOSABLE_HARNESS.md](../harness/COMPOSABLE_HARNESS.md) (Office iteration plan and DEV_NOTES in local `doc_Private/docs/`)

---

## 1. Product One-Liner

Zagens Office line is not "another chat that writes Word", but:

> **A local desktop office copilot that can read spreadsheets, search the web, and deliver files (DOCX / XLSX / PPTX / PDF)** — brief first, then deliverables; voice can be added later (STT → TTS briefing → confirm → document).

Division of labor with **Code mode**: Office trims shell / patch / sub-agent and other code-oriented tools, keeps `read_office`, `write_office`, web access, and skills (see `office.md`).

---

## 2. Scenario Taxonomy (Two Views)

### 2.1 By Information Flow

| Type | Meaning | Typical Output | Zagens Primary Capabilities |
|------|---------|----------------|----------------------------|
| **External scan** | Market, competitors, policy, quotes | Research brief DOCX | `web_search`, `fetch_url`, `finance` |
| **Internal aggregation** | Collect department materials into one executive view | Operations daily/weekly DOCX | `read_office` multi-file + summary + `write_office` |
| **Internal ops** | Shop floor, sales, finance tables from internal data | Production brief, quote XLSX | `read_office` + `write_office` (XLSX pure Rust) |
| **Creation** | Draft proposals, contracts, minutes from scratch | DOCX / PPTX | `load_skill` + `write_office` |
| **Processing** | Translate, merge, revise | Same or new format | `read_office` → edit → `write_office` / `load_office_payload` |
| **Delivery** | Find file, preview, edit column, send out | Experience layer | `deliverables/` default dir + highlight, PDF/HTML right-panel preview, open with system app; one-click PDF export TBD |

### 2.2 By Role / Function

See §4 scenario catalog (expanded by department).

### 2.3 By Four Orthogonal Dimensions (Unified Abstraction · Recommended as Architecture First-Class)

§2.1 / §2.2 are human-facing "catalog views". From an **engineering view**, the long §4 list is just different values on four **mutually independent** axes. Any office scenario can be written as coordinates `(ingest, transform, render, loop)`:

| Axis | Meaning | Value Space | §2.1 / §4 Manifestation |
|------|---------|-------------|-------------------------|
| **① Ingest** | Where data comes from | `web` (online) · `files` (`inbox/`,`data/`) · `dictation` (spoken) · `vision` (`describe_image`, scans) · `connector` (ERP/CRM, **future**) | External scan / aggregation / internal ops |
| **② Transform** | What to do with data | `summarize` · `aggregate` · `compare` · `compute` · `translate` · `draft` · `extract` | Summary / comparison / quote calc / translation / drafting |
| **③ Render** | What to deliver | Format `docx/xlsx/pptx/pdf` + `sections`/`sheets` structure | §5 default output column |
| **④ Loop** | Interaction rhythm | `oneshot` · `brief_first` (brief first) · `confirm` · `iterable` (incremental edit) · `voice` | §3 pipeline + Phase C voice |

**Key conclusion:** "Scenario" should not be architecture first-class; **the four axes are**. One scenario = one coordinate set on four axes; unified architecture only needs reusable blocks per axis; new scenarios reduce to **declarative config** (see §3 skill contract).

**Re-explaining document comparisons with four axes:**

- **Executive daily vs shop-floor morning report (§4.1):** Difference only in `ingest` (multi-source vs single-source) + `render.sections`; pipeline kernel identical.
- **P0-1 ~ P0-4 four demos (§6):** Exactly four representative coordinates on four axes (external scan / aggregation / internal read-table / compute+iterable) — **running these 4 ≈ validating each axis**, not four isolated features.
- **Voice (Phase C):** Only swaps `loop` from `brief_first(text)` to `voice(STT/TTS)`; **does not touch ingest/transform/render**.

---

## 3. Unified Architecture (Shared by All Scenarios)

Office line converges to **4 layers**: scenarios vary only at the top (declarative config); bottom three layers fixed and reused.

```
┌─────────────────────────────────────────────────────────────┐
│ L4  Scenario layer (declarative) = one SKILL.md "contract"  │
│     Declares four-axis coordinates: ingest+transform+render+loop+verify │
│     ↑ New scenarios at this layer, zero engine changes (§3.2) │
├─────────────────────────────────────────────────────────────┤
│ L3  Pipeline kernel (fixed 6 stages, §3.1)                  │
│     Trigger → ingest → brief → confirm → generate → deliver/iterate │
├─────────────────────────────────────────────────────────────┤
│ L2  Capability primitives (orthogonal tools, shipped/in progress) │
│   Ingest read_office · web_search · fetch_url · finance          │
│   Vision describe_image (vision bridge, scan OCR path)              │
│   Generate write_office(source direct) · load_office_payload(edit) │
│   Deliver deliverables/ · preview · open_with_system_app             │
│   P0 engineering gaps → office-mode-iteration-plan §3 capability matrix │
├─────────────────────────────────────────────────────────────┤
│ L1  Foundation TaskType=Office isolation · Python venv · network policy gate │
└─────────────────────────────────────────────────────────────┘
```

> **Current note:** L1/L2/L3 landed (see [office-mode-iteration-plan.md](../../doc_Private/docs/office-mode-iteration-plan.md) recommended implementation order 1–8). **Remaining work concentrates on L4 scenario fill and acceptance**, plus §8 L2 items (read-table fidelity golden, iterative edit productization, enterprise templates); scan OCR **not built-in engine**, unified via vision bridge (§4.6).

### 3.1 Pipeline Kernel (L3, 6 Fixed Stages)

**Logical order (user story):** ingest → transform (model reasoning) → brief → confirm → generate → deliver/iterate.  
The 6 stages below are **product acceptance segments**; implementation need not be a hard state machine; model may interleave "read + think + write" in one turn.

```
Trigger (task card / typing / future: voice STT)
  → Office mode session (independent TaskType, isolated from Code sessions)
  → load_skill (optional, recommended for high-frequency tasks)
  → ① Ingest: read_office / web_search / fetch_url / user dictation / describe_image (scans)
  → ② Transform: summarize / aggregate / compare / compute / translate / draft (prompt + model, not independent runtime operator)
  → ④ Brief (optional): conversation summary (future: TTS 30–60s)     ← Loop.brief_first
  → ④ Confirm (optional): "Generate formal document?"                 ← Loop.confirm
  → ③ Generate: write_office → deliverables/<title>.<ext>             ← Render
  → ④ Deliver/iterate: preview / send / load_office_payload             ← Loop.iterable
```

### 3.2 Skill Contract (L4, New Scenario = Fill Form)

Unify the existing **11** bundled skills and appendix A pending skills into one declaration schema. Each `office-*/SKILL.md` only fills four axes + verification; **no engine changes**. Example (P0-2 template "Executive Operations Daily Brief"):

```yaml
id: office-executive-daily-brief
ingest:                       # Axis ① Ingest
  - kind: files
    from: inbox/              # multi-department attachments
    formats: [docx, xlsx, pdf]
transform:                    # Axis ② Transform (L2 + model reasoning)
  - summarize_per_source
  - aggregate
  - extract: pending_decisions
render:                       # Axis ③ Render
  format: docx
  sections: [Overview, Department Highlights, Risks, Pending Decisions, Appendix]
  out: deliverables/          # path optional, default here
loop:                         # Axis ④ Loop
  brief_first: true           # text/voice brief first
  confirm_before_render: true
  iterable: true              # supports load_office_payload incremental edit
verify:                       # acceptance config (see "contract landing" below)
  - sources_cited
  - has_section: Pending Decisions
```

**Contract field reference:**

| Block | Meaning | Runtime |
|-------|---------|---------|
| `ingest` / `render` / `loop` | Four-axis coordinates + directory conventions | SKILL body steps + model execution; engine does not parse YAML |
| `transform` | **Skill instruction semantics** (aggregate, compare, compute…) | **Not** independent runtime operator; written in SKILL steps |
| `verify` | Demo / regression oracle | **Not** auto gate; for manual acceptance or future headless scripts |

**Contract landing (three phases, none require engine changes):**

| Phase | Action |
|-------|--------|
| **1 — Convention** | ✅ **11/11** — all bundled `office-*/SKILL.md` include `## Skill Contract` + YAML + numbered steps (template: `office-executive-daily-brief`) |
| **2 — Lint** | ❌ TBD — optional `scripts/office-skill-lint.mjs`: check contract fields, §6 acceptance items complete |
| **3 — Regression** | ⚠️ Partial — `fixtures/harness/office-demo/` + `scripts/office-demo-oracle.ps1` (P0-2/3/4; P0-1 no headless oracle) |

> **Alignment with existing skills:** All 11 bundled skills already follow "confirm → ingest → transform → generate → incremental edit" (see `office-weekly-report`). Contract **explicitizes** implicit conventions; P0 three new skills and cards landed, see §5 / §10.

**Directory conventions (recommended demo / enterprise workspace):**

| Path | Purpose |
|------|---------|
| `inbox/` | Raw attachments from departments (dailies, sheets, minutes); **user-created** or copied from `office-demo` fixtures |
| `data/` | Structured data sources (price lists, production dailies, master data); same, **not auto-initialized** |
| `deliverables/` | Agent output (default, skill may omit `path`); ensured on workspace create |
| `templates/` | Enterprise master templates / price list templates (future) |

**Voice extension (Phase C, out of scope this phase):** Same pipeline, only swap "trigger + brief" for STT / TTS; execution still Office tool surface. See `doc_Private/docs/desktop/DEV_NOTES.md` §seating briefing.

**vs LHT / CRAFT:** Office single tasks usually **do not need** LHT checklist; multi-file long research, cross-day follow-up may use light checklist, not Office default.

---

## 4. Scenario Catalog

Legend: **Maturity** — ✅ skill/card tryable · ⚠️ skill exists but E2E or read-table fidelity pending · ❌ skill TBD or needs enterprise template · 🔮 future (out of scope: voice, ERP/CRM connector)

**Four-axis shorthand (§2.3):** `ingest|transform|render|loop` — e.g. `files,aggregate,docx,brief+confirm`

### 4.1 Management / Decision

| Scenario | Four-axis (shorthand) | Role | Typical Prompt / Trigger | Input | Output | Skill / Card | Maturity |
|----------|----------------------|------|--------------------------|-------|--------|--------------|----------|
| **Operations daily rollup** | `files,aggregate,docx,brief+confirm` | Executive, leadership | "Summarize yesterday's dailies" | `inbox/` dept DOCX/summaries | Executive brief DOCX | `office-executive-daily-brief` ✅ | ⚠️ |
| **Weekly / monthly report** | `files+dictation,summarize,docx,iterable` | Manager | "Write this week's weekly report" | Attachments + dictation | DOCX | `office-weekly-report` ✅ | ✅ |
| **Project report PPT** | `files+dictation,draft,pptx,oneshot` | Project lead | "Make a project report PPT" | Bullet points + materials | PPTX | `office-project-report` ✅ | ✅ |
| **Monthly operations analysis** | `files,compute+compare,xlsx+docx,iterable` | Finance + leadership | "Operations analysis from last month's sales sheet" | XLSX | DOCX + chart XLSX | `office-data-report` + custom | ⚠️ |
| **Decision memo** | `files+web,compare,docx,confirm` | Executive | "Organize A/B options for decision" | Notes + research | DOCX (option comparison) | TBD `office-decision-memo` | ❌ |
| **Board / investor brief** | `files,summarize+extract,pptx,brief` | CEO | "Compress to 5-page investor highlights" | Long materials | PPTX / DOCX | TBD | ❌ |

**Executive daily vs shop-floor morning report:** Executive scenario is **multi-source rollup + pending decisions**; shop-floor is **single topic + structured metrics** (see §4.3).

---

### 4.2 Marketing / Sales / Business

| Scenario | Four-axis (shorthand) | Role | Typical Prompt | Input | Output | Skill | Maturity |
|----------|----------------------|------|----------------|-------|--------|-------|----------|
| **Competitor / market dynamics** | `web,summarize+compare,docx,oneshot` | Marketing | "Research competitor A/B recent moves" | Web | DOCX + sources | `office-competitive-analysis` ✅ | ✅ |
| **Market daily / weekly** | `web+files,summarize,docx,oneshot` | Marketing | "What's moving in the industry today" | Web + optional internal notes | DOCX | `office-market-watch` (TBD) | ⚠️ |
| **Campaign / battle brief** | `web+dictation,draft,docx,confirm` | Marketing | "Write Q3 promotion plan outline" | Dictation + research | DOCX | TBD | ❌ |
| **Customer quote** | `files,compute,xlsx,iterable` | Sales | "Quote per customer requirements" | Price list XLSX + requirements | Quote XLSX | `office-customer-quote` ✅ | ⚠️ |
| **Business proposal** | `files+dictation,draft,docx+pptx,confirm` | Sales | "Write proposal for customer" | Requirements + template | DOCX / PPTX | TBD | ❌ |
| **Sales daily** | `files,aggregate,docx+xlsx,oneshot` | Sales manager | "Summarize today's sales follow-ups" | CRM export / sheet | DOCX / XLSX | TBD | ❌ |
| **Contract first draft** | `dictation,draft,docx,iterable` | Business / legal assist | "Draft procurement contract first draft" | Clause bullet points | DOCX | `office-contract-draft` ✅ | ✅ |
| **RFP response outline** | `files,extract+draft,docx,confirm` | Pre-sales | "Response outline per RFP document" | PDF/DOCX RFP | DOCX outline | TBD | ❌ |

---

### 4.3 Production / Quality / Supply Chain / Operations

| Scenario | Four-axis (shorthand) | Role | Typical Prompt | Input | Output | Skill | Maturity |
|----------|----------------------|------|----------------|-------|--------|-------|----------|
| **Production + quality morning report** | `files,summarize+aggregate,docx+xlsx,brief+confirm` | Production/quality | "Report yesterday's production and quality status" | Yesterday MES/Excel export | Brief first → DOCX/XLSX | `office-production-daily-report` ✅ | ⚠️ |
| **Incident / 8D report** | `files,draft+extract,docx,confirm` | Quality | "Document this batch defect incident" | Inspection records | DOCX | TBD | ❌ |
| **Scheduling / work order summary** | `files,summarize,docx,oneshot` | Planning | "Summarize this week's work order completion" | XLSX | DOCX | TBD | ❌ |
| **Supplier evaluation** | `files,compare,xlsx,iterable` | Procurement | "Compare three suppliers on price and lead time" | Multiple XLSX | Comparison XLSX | TBD | ⚠️ |
| **Inventory / turnover brief** | `files,compute+compare,xlsx,iterable` | Warehouse | "Last week inventory movement explanation" | Inventory sheet | XLSX report | `office-data-report` variant | ⚠️ |
| **SOP / work instruction** | `files+dictation,draft,docx,iterable` | Process engineering | "Write SOP for this process" | Dictation + old version | DOCX | TBD | ❌ |

---

### 4.4 Finance / Admin / HR

| Scenario | Role | Typical Prompt | Input | Output | Skill | Maturity |
|----------|------|----------------|-------|--------|-------|----------|
| **Expense / reimbursement rollup** | Finance | "Summarize this month's expense categories" | XLSX | XLSX + summary DOCX | TBD | ⚠️ |
| **Budget variance** | Finance | "Actual vs budget variance explanation" | Two XLSX | DOCX + table | TBD | ⚠️ |
| **Invoice / reconciliation list** | Finance | "Organize pending payment list" | CSV/XLSX | XLSX | TBD | ⚠️ |
| **Meeting minutes** | Admin | "Organize today's meeting resolutions" | Transcript / notes | DOCX | `office-meeting-minutes` ✅ | ✅ |
| **Notice / announcement** | Admin | "Write company-wide holiday notice" | Dictation | DOCX | Generic office | ✅ |
| **Job description** | HR | "Write Java engineer JD" | Role bullet points | DOCX | TBD | ❌ |
| **Interview notes** | HR | "Organize candidate interview evaluation" | Notes | DOCX | TBD | ❌ |
| **Resume / cover letter** | Individual / HR | "Tailor resume to role" | Old resume | DOCX | `office-resume` ✅ | ✅ |

---

### 4.5 Product / R&D / Project (Office-leaning, Not Code Mode)

| Scenario | Role | Typical Prompt | Input | Output | Skill | Maturity |
|----------|------|----------------|-------|--------|-------|----------|
| **Release notes** | Product | "Write version release notes" | changelog | DOCX | `office-release-notes` ✅ | ✅ |
| **PRD outline** | Product | "Organize requirements into PRD structure" | Notes | DOCX | TBD | ❌ |
| **User research summary** | Product | "Summarize 5 interview transcripts" | Multiple DOCX | DOCX | TBD | ⚠️ |
| **Competitor feature matrix** | Product | "Build feature comparison table" | Web + internal | XLSX / DOCX | `office-competitive-analysis` extended | ⚠️ |

> **Boundary:** Change code, run tests, long-horizon refactor → **Code mode + LHT/CRAFT**; Office only delivers **documents / sheets / reports**.

---

### 4.6 General / Cross-Functional

| Scenario | Description | Maturity |
|----------|-------------|----------|
| **Multi-document merge** | Three weeklies → one monthly | ⚠️ read-table fidelity + multi-file |
| **Translation / localization** | Contract / PPT section translation | ⚠️ |
| **Format conversion narrative** | "Turn bullet points into PPT" | ✅ `write_office` |
| **Email / message draft** | External reply, follow-up email | ✅ conversation OK, optional DOCX |
| **Policy / regulation summary** | Web + citations | ⚠️ source discipline |
| **Data visualization** | CSV → chart XLSX / PPTX | ✅ `write_office` `source` direct CSV/TSV/XLSX |
| **Scan OCR** | Scanned PDF, invoice images | ✅ **Vision bridge** — `read_office` empty text layer → `describe_image` (no built-in OCR in `read_office`; see [office-read-tool-plan.md](../../doc_Private/docs/office-read-tool-plan.md)) |

---

## 5. Landed Skills and UI Cards (Cross-Reference)

**11** bundled skills, **11** empty-state task cards (P0 three pinned top). Paths: `crates/runtime-server/assets/skills/office-*/SKILL.md`; cards and prefill: `crates/desktop/web-ui/src/components/OfficeEmptyState.tsx` + `i18n/locales/*.ts` → `officeEmpty`.

| Skill | UI Card | Default Output | Batch |
|-------|---------|----------------|-------|
| `office-executive-daily-brief` | Executive Daily Brief Rollup | DOCX | P0 |
| `office-customer-quote` | Customer Quote | XLSX | P0 |
| `office-production-daily-report` | Production & Quality Morning Report | DOCX | P0 |
| `office-weekly-report` | Weekly Report | DOCX | First batch |
| `office-meeting-minutes` | Meeting Minutes | DOCX | First batch |
| `office-project-report` | Project Report PPT | PPTX | First batch |
| `office-data-report` | Data Report | XLSX | First batch |
| `office-competitive-analysis` | Competitive Analysis | DOCX | First batch |
| `office-contract-draft` | Contract First Draft | DOCX | First batch |
| `office-resume` | Resume / Cover Letter | DOCX | First batch |
| `office-release-notes` | Release Notes | DOCX | First batch |

---

## 6. Priority Demo Scenarios (Suggested P0)

Combining business discussion, suggest **4 end-to-end demos first** (text version, no voice) to prove Office line business value. These 4 **deliberately pick one representative coordinate per axis** — green runs = each axis validated, not four isolated features (see §2.3):

| Priority | Scenario | Primary Axis Validated | Skill / Card | Acceptance Criteria | Landing Status |
|----------|----------|------------------------|--------------|---------------------|----------------|
| **P0-1** | Market competitor / industry dynamics | ① `ingest=web` + source constraints | `office-competitive-analysis` ✅ (`office-market-watch` not built) | Web + source list + DOCX in `deliverables/` | ⚠️ tryable; no headless oracle |
| **P0-2** | Executive operations daily rollup | ① `ingest=files(multi)` + ② `aggregate` | `office-executive-daily-brief` ✅ | `inbox/` multi-attach → 5-section structure + pending decisions | ✅ skill+card+fixtures+oracle |
| **P0-3** | Production/quality morning report | ① `ingest=files` + ④ `brief_first` | `office-production-daily-report` ✅ | Read yesterday XLSX → text brief → DOCX/XLSX | ⚠️ skill chain ready; read-table fidelity affects stability |
| **P0-4** | Customer quote | ② `compute` + ④ `loop=iterable` | `office-customer-quote` ✅ | Price list + requirements → tax-inclusive total XLSX, incremental price edit | ⚠️ skill+fixtures+oracle; iterative price UX TBD |

**Shared experience P0 (engineering, landed):** See [office-mode-iteration-plan.md](../../doc_Private/docs/office-mode-iteration-plan.md) implementation order 1–8 — `read_office` (calamine), default `deliverables/` + highlight, PDF/HTML right-panel preview, open with system app, `load_office_payload`, `write_office` `source` direct feed.

---

## 7. Phased Roadmap

### Phase A — Text Closed Loop (current, ~90%)

- ✅ 11 skills + 11 cards + P0 fixtures + oracle (P0-2/3/4)  
- ✅ No STT/TTS dependency  
- Goal: any scenario **one sentence → downloadable file**  
- **Remaining:** P0 E2E green runs routine, `office-skill-lint`, appendix A remaining 5 skills, read-table fidelity golden

### Phase B — Data and Iteration (future, parts out of scope below)

- `inbox/`, `data/` directory conventions — user-created or copy fixtures (**not auto-initialized**)  
- 🔮 MCP to ERP / CRM / announcements (optional, **not this phase**)  
- ⚠️ `load_office_payload` incremental quote/report edit — tool exists, flow productization TBD  
- 🔮 Scheduled tasks (background automation) for "daily competitor digest"

### Phase C — Voice (seating briefing, out of scope this phase)

- STT / TTS trigger and spoken brief — **not this phase**; architecturally only replaces `loop`, does not touch ingest/transform/render  
- See `doc_Private/docs/desktop/DEV_NOTES.md` §seating briefing

---

## 8. Capability Gaps (Office Line Cross-Cutting)

> **Architecture meaning:** Gaps below land on **L2 capability primitives** (§3) or **L4 acceptance**, decoupled from specific scenarios. Fix one primitive, all scenarios using that axis benefit.  
> **Not counted as gaps this phase:** STT/TTS (Phase C), ERP/CRM connector, `inbox/`/`data/` auto-init.

| Gap | Affected Scenarios | Status | Reference |
|-----|-------------------|--------|-----------|
| XLSX read fidelity (numFmt golden etc.) | Production, quote, finance, executive rollup | ⚠️ `read_office`+calamine shipped, stability pending | office-mode-iteration-plan §P0 R1 |
| Iterative edit productization | Quote price change, report column change | ⚠️ `load_office_payload` tool exists, UX/skill steps TBD | office-mode-iteration-plan §P0-4 |
| Enterprise templates | Quote, contract, brief | ❌ `templates/` convention only | §G |
| One-click PDF export | External send | ❌ backlog | office-mode-iteration-plan §15 |
| Round-trip hand-edited files | User-edited docx/xlsx re-edit | ❌ payload cache path only | office-mode-iteration-plan §P0-4 advanced |
| Source / hallucination constraints | Market, competitor, policy | ⚠️ `office-competitive-analysis` skill requires sources | §11 |
| Scan OCR | Scanned PDF, invoice images | ✅ **Vision bridge** — `describe_image`; `read_office` not built-in | office-read-tool-plan §OCR |
| Post-generate preview / highlight | All | ✅ default `deliverables/` + highlight; PDF/HTML right panel; open with system app | office-mode-iteration-plan §F |
| P0 E2E oracle routine | Four-axis validation | ⚠️ `office-demo-oracle.ps1` has P0-2/3/4 | §6 |

---

## 9. Relation to Commercialization / Potential

- **Coding harness (LHT/CRAFT)** → developer reputation, long-horizon reliability  
- **Office scenario map (this doc)** → non-developer understandable, demoable, industry-packable (manufacturing, trade, marketing teams)  
- Commercialization can expand along **"Skill template packs + data connectors + voice briefing"**, compatible with BYOK  

---

## 10. Follow-Up Docs and Implementation Entry Points

| Action | Location |
|--------|----------|
| New skill | `crates/runtime-server/assets/skills/office-<name>/SKILL.md` (include `## Skill Contract`) |
| **P0 template skill** | [`office-executive-daily-brief`](../../crates/runtime-server/assets/skills/office-executive-daily-brief/SKILL.md) (contract schema reference) |
| Task cards (11) | `OfficeEmptyState.tsx` + `web-ui/src/i18n/locales/*.ts` → `officeEmpty` |
| Office capability iteration | [office-mode-iteration-plan.md](../../doc_Private/docs/office-mode-iteration-plan.md) |
| Demo fixtures | [`fixtures/harness/office-demo/`](../../fixtures/harness/office-demo/README.md) |
| P0 oracle | [`scripts/office-demo-oracle.ps1`](../../scripts/office-demo-oracle.ps1) |
| Contract lint (TBD) | `scripts/office-skill-lint.mjs` (optional) |
| Voice (Phase C, out of scope) | `doc_Private/docs/desktop/DEV_NOTES.md` §2026-05-18 seating briefing |

---

## Appendix A: Suggested Skill Naming (Including P0 Built)

| Skill ID | Scenario | Status |
|----------|----------|--------|
| `office-executive-daily-brief` | Executive operations daily | ✅ P0-2 |
| `office-production-daily-report` | Production + quality morning report | ✅ P0-3 |
| `office-customer-quote` | Customer quote | ✅ P0-4 |
| `office-market-watch` | Market daily / industry dynamics | ❌ TBD (P0-1 uses `office-competitive-analysis` for now) |
| `office-sales-daily` | Sales daily | ❌ TBD |
| `office-decision-memo` | Decision memo | ❌ TBD |
| `office-incident-report` | Quality / operations incident report | ❌ TBD |
| `office-proposal` | Business proposal | ❌ TBD |

Naming consistent with existing `office-*`; description line notes format and default `deliverables/`.

**Authoring:** Each new skill fills four axes + `verify` per §3.2 skill contract; no new engine logic. Use `office-executive-daily-brief` as template; P0 three landed same structure, remaining 5 per appendix.
