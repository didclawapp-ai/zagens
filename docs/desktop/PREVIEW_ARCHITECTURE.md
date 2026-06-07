# Zagens desktop — document preview architecture

> **Status (2026-05):** Phase 1 landed in Zagens (`crates/desktop` + `web-ui`). This document is the **architecture reference, constraint index, and Phase 2 backlog**.

## Implementation summary

| Scope | Description |
|-------|-------------|
| Frontend module | `crates/desktop/web-ui/src/components/preview/` — `types`, `detector`, `PreviewContainer`, `PreviewDispatcher`, `renderers/*` |
| Entry | `RightPanel.tsx` — workspace file click → overlay; text via runtime API, binary via Tauri |
| Binary read | Tauri **`read_thread_workspace_binary`** (`crates/desktop/src/commands.rs`): `thread_id` + `relative_path`; Rust calls `GET /v1/threads/:id` for workspace root, then `canonicalize` + root prefix check (same rules as runtime) |
| CSP | `tauri.conf.json` includes `img-src 'self' data:` |
| Markdown compat | `MarkdownPreview.tsx` → re-exports `MarkdownRenderer` only, `@deprecated` |
| PDF / Office | **Binary readable + placeholder UI** (`OfficePlaceholder`); embedded rendering is Phase 2 |
| Truncation | `PreviewState.truncated`; `ImageRenderer` and placeholders show &gt;10 MiB truncation notice |

## Background constraints (runtime API, still apply)

| Constraint | Location | Impact |
|------------|----------|--------|
| Non-UTF-8 rejected | `crates/runtime-server/src/runtime_api/` — `workspace/file` uses `String::from_utf8` | Images / PDF / Office **cannot** use that HTTP endpoint for body |
| Text cap 512 KB | `MAX_WORKSPACE_FILE_BYTES` | Very large plain text needs constant change or separate desktop read path (not done) |
| `language_hint` | Server `language_from_name` | Frontend **`detectFileType`** prefers hint; `markdown` → `MarkdownRenderer` |

## Supported formats and renderers

| Format | Renderer | Notes |
|--------|----------|-------|
| Markdown (`.md` / `.mdx`, etc.) | `MarkdownRenderer` | markdown-it + DOMPurify |
| Code / text with `language_hint` | `CodeRenderer` | highlight.js |
| Plain text / unknown | `TextRenderer` | HTML escape |
| Images | `ImageRenderer` | `data:{mime};base64,...` |
| CSV / TSV | `CsvRenderer` | HTML `<table>` |
| PDF / Office | **Placeholder** | Shows file size; extra note when `truncated` |

## Data flow (implemented)

```
User clicks file
  ├→ detectFileType(title) (binary routing; no language_hint)
  ├→ isBinaryFileType → invoke('read_thread_workspace_binary', { threadId, relativePath })
  │     (Rust: Bearer to local runtime → thread.workspace + safe relative path join)
  └→ else readThreadWorkspaceFile(threadId, relPath)
```

## Binary read (`read_thread_workspace_binary`)

- **Args:** `thread_id`, `relative_path` (same semantics as HTTP `workspace/file` `path`; `..` forbidden).
- **Workspace root:** `http://127.0.0.1:{port}/v1/threads/{id}`, parse JSON `thread.workspace` (same as browse API).
- **Read cap:** `PREVIEW_MAX_BINARY_BYTES` (10 MiB); truncate with `truncated: true`; `sniff_mime` for MIME.
- **Deps:** `base64`, `reqwest`, `serde_json` (`crates/desktop/Cargo.toml`).

## Phase 1 checklist — done

| # | Task | Status |
|---|------|--------|
| 1–15 | Preview module, CSP, hljs, `read_thread_workspace_binary`, path checks, truncated UI | ✅ |

## Phase 2 (not scheduled)

| Item | Description |
|------|-------------|
| `PdfRenderer` / `OfficeRenderer` | pdf.js, SheetJS / mammoth, etc.; CSP extensions |
| Clickable paths in chat / tools | Product increment |

### Office / PDF options (notes)

- **Office.js:** Add-in host API, not a general local docx embed.
- **Office Online iframe:** Often needs a reachable URL; pure local workspace needs extra architecture.
- **React-friendly:** SheetJS, mammoth + DOMPurify, separate pptx POC or commercial viewer.

## Security constraints

| Constraint | Description |
|------------|-------------|
| Paths | Only **runtime workspace root for `thread_id`** + relative path; no `..` or escape |
| Auth | Thread fetch uses **same Bearer token as sidecar** (`AppContext.runtime_token`) |
| Traversal | `canonicalize` + `starts_with(workspace_base)` (aligned with `runtime_api.rs` `safe_thread_subpath`) |
| Size | Text 512 KB (API); binary preview reads up to 10 MiB |

---

**Dependency:** Desktop shell must reach local runtime; if binary preview fails, check sidecar readiness and thread existence.
