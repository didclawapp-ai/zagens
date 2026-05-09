// ---------------------------------------------------------------------------
// File-type detector — pure function, zero React dependency.
//
// Strategy (mirrors server-side `language_from_name` in
// `crates/tui/src/runtime_api.rs`):
//   1. If the runtime API returned a `language_hint`, the file was
//      successfully read as UTF-8 text → FileType.Code.
//   2. Otherwise, look up the file extension in EXT_MAP.
//   3. Fallback → FileType.Text (plain text / unknown).
//
// The EXT_MAP below is only authoritative for *binary* routing decisions
// (Image / Pdf / Office) and text formats not covered by the server hint.
// For code files the `language_hint` from the API takes priority.
// ---------------------------------------------------------------------------

import { FileType } from './types';

const EXT_MAP: Record<string, FileType> = {
  // Markdown
  '.md': FileType.Markdown,
  '.markdown': FileType.Markdown,
  '.mdx': FileType.Markdown,

  // Code – extension is used for routing; language selection prefers API hint
  '.rs': FileType.Code,
  '.ts': FileType.Code,
  '.tsx': FileType.Code,
  '.js': FileType.Code,
  '.jsx': FileType.Code,
  '.mjs': FileType.Code,
  '.cjs': FileType.Code,
  '.json': FileType.Code,
  '.toml': FileType.Code,
  '.yaml': FileType.Code,
  '.yml': FileType.Code,
  '.py': FileType.Code,
  '.sh': FileType.Code,
  '.bash': FileType.Code,
  '.css': FileType.Code,
  '.html': FileType.Code,
  '.htm': FileType.Code,
  '.sql': FileType.Code,
  '.xml': FileType.Code,
  '.java': FileType.Code,
  '.go': FileType.Code,
  '.c': FileType.Code,
  '.h': FileType.Code,
  '.cpp': FileType.Code,
  '.cc': FileType.Code,
  '.hpp': FileType.Code,
  '.swift': FileType.Code,
  '.kt': FileType.Code,
  '.scala': FileType.Code,
  '.rb': FileType.Code,
  '.php': FileType.Code,
  '.lua': FileType.Code,
  '.r': FileType.Code,
  '.vue': FileType.Code,
  '.svelte': FileType.Code,

  // Images (binary — must go through Tauri read_thread_workspace_binary)
  '.png': FileType.Image,
  '.jpg': FileType.Image,
  '.jpeg': FileType.Image,
  '.gif': FileType.Image,
  '.svg': FileType.Image,
  '.webp': FileType.Image,
  '.bmp': FileType.Image,
  '.ico': FileType.Image,

  // Structured text
  '.csv': FileType.Csv,
  '.tsv': FileType.Csv,

  // Binary documents (Phase 2 renderers; binary routing applies now)
  '.pdf': FileType.Pdf,
  '.docx': FileType.Office,
  '.xlsx': FileType.Office,
  '.pptx': FileType.Office,
  '.doc': FileType.Office,
  '.xls': FileType.Office,
  '.ppt': FileType.Office,

  // Plain text
  '.txt': FileType.Text,
  '.log': FileType.Text,
};

/**
 * Resolve the `FileType` for a file.
 *
 * When the runtime API returns a `language_hint` the file passed UTF-8
 * validation — treat it as Code (the hint carries the exact language).
 */
export function detectFileType(
  fileName?: string,
  languageHint?: string | null,
): FileType {
  // 1. Server-side language hint → definite text file with known language
  if (languageHint) {
    // Markdown files get a language hint of "markdown" from the server;
    // route them to the Markdown renderer for full markdown-it treatment.
    if (languageHint === 'markdown') {
      return FileType.Markdown;
    }
    return FileType.Code;
  }

  // 2. Extension lookup
  if (fileName) {
    const dot = fileName.lastIndexOf('.');
    if (dot !== -1) {
      const ext = fileName.slice(dot).toLowerCase();
      const mapped = EXT_MAP[ext];
      if (mapped !== undefined) {
        return mapped;
      }
    }
  }

  // 3. Fallback — assume plain text
  return FileType.Text;
}

/**
 * Returns `true` when the file type requires the Tauri `read_thread_workspace_binary`
 * command (the runtime API rejects non-UTF-8 content).
 */
export function isBinaryFileType(ft: FileType): boolean {
  return ft === FileType.Image || ft === FileType.Pdf || ft === FileType.Office;
}
