# Deliverables preview & open

Office outputs land in **`deliverables/`** — DOCX, XLSX, PPTX, or PDF written by `write_office`.

## When the folder appears

`deliverables/` is **not** created at workspace setup; the runtime creates it on the **first `write_office` write**. You may create an empty folder yourself.

## View in the workspace

1. Sidebar → **Workspace**
2. Pick the **Deliverables** preset, or expand the `deliverables/` node
3. Click a file — the right panel shows an [extracted-text preview](/docs/workspace/preview)

New files are often **highlighted** after a task finishes.

## Open in system apps

For full layout, formulas, or animations:

- File tree context menu → **Open with system app** (`open_with_system_app`)
- Or the same action from the preview area

Word / Excel / PowerPoint launch with your OS defaults.

## Incremental edits

The agent can `load_office_payload` on an existing file, change sheets / blocks / slides, then **`write_office` to the same path**. Review in preview before asking for another pass.

## Acceptance tips

- Numbers should match source spreadsheets and cited web sources (see [P0 demos](/docs/office/scenarios))
- Recheck complex Excel formulas in Excel

Related: [Office workspace](/docs/office/workspace) · [Office I/O](/docs/tools/office-io) · [File tree](/docs/workspace/file-tree)
