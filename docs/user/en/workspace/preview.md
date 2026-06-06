# File preview

Click a file in the workspace tree to open a **read-only preview** in the right panel (format-dependent).

## Supported previews

| Type | Behavior |
|------|----------|
| **Code & Markdown** | Syntax-friendly text view |
| **Images** | Inline image render |
| **CSV** | Table-style view |
| **Office & PDF** | Extracted text preview (DOCX, XLSX, PPTX, PDF) |
| **Mermaid** | Diagram render when detected |
| **Binary** | Hex snippet for unknown binaries |

Office files can also be opened in the system default app from the workspace UI.

## Diffs

After `edit_file` or `apply_patch`, Zagens shows a **diff2html** comparison so you can review changes before accepting.

## Deliverables (Office)

Generated files usually land in `deliverables/`. The preview panel highlights new outputs when the agent finishes `write_office`. See [Deliverables](/docs/office/deliverables).

## Limits

- Preview is for inspection — editing happens through agent tools, not a full IDE.
- Very large files may truncate in preview; ask the agent to read specific sections.

Related: [Workspace overview](/docs/workspace/overview) · [Deliverables](/docs/office/deliverables)
