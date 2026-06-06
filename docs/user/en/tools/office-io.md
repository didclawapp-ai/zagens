# Office I/O

Office deliverables are produced with **`read_office`** and **`write_office`** (plus `read_file` for some formats).

## `read_office`

Extracts text/tables from:

- Word (`.docx`)
- Excel (`.xlsx`)
- PowerPoint (`.pptx`) — slide text
- PDF — when enabled

Use on files under `inbox/` or `data/` in the [office workspace](/docs/office/workspace).

## `write_office`

Creates or updates structured Office files:

| Format | Typical output |
|--------|----------------|
| DOCX | Reports, briefs, minutes |
| XLSX | Quotes, data tables |
| PPTX | Slide decks |

Outputs land in `deliverables/` — preview and [open in system apps](/docs/office/deliverables) from the Office sidebar.

## `load_office_payload`

For files already written, the runtime caches structured JSON. **Incremental edit** flow:

1. `load_office_payload` — fetch cached sheets / blocks / slides
2. Modify the structure
3. `write_office` — **overwrite the same path**

Use for “fix one quote column” or “add a minutes paragraph” without regenerating from scratch.

## Other Office tools

Office mode also exposes `list_dir`, `write_file` (plain text), `glob_files`, `file_search`, and helpers; deliverables still center on `read_office` / `write_office`.

## Skills

Bundled [Office skills](/docs/office/skills) call these tools after `load_skill`. P0 guides: [competitive](/docs/office/p0-competitive), [executive](/docs/office/p0-executive), [production](/docs/office/p0-production), [quote](/docs/office/p0-quote).

## Limits

- Complex Excel formulas may need manual verification in Excel.
- Branding/templates are best supplied in `data/` as reference files.
- Code mode uses file tools instead — switch task type for engineering work.

Related: [Web tools](/docs/tools/web) · [File tools](/docs/tools/files)
