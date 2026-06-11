// ---------------------------------------------------------------------------
// Preview type system — shared by all renderers, the dispatcher, and RightPanel
// ---------------------------------------------------------------------------

export enum FileType {
  Markdown = 'markdown',
  Code = 'code',
  Text = 'text',
  Image = 'image',
  Csv = 'csv',
  Pdf = 'pdf',
  Html = 'html',
  /** Legacy routing label; DOCX/PPTX/XLSX are opened externally, not previewed. */
  Office = 'office',
  Unknown = 'unknown',
}

export interface PreviewState {
  /** Breadcrumb / display name shown in the overlay header. */
  title: string;
  /**
   * Workspace-relative path of this file (POSIX slashes), e.g. `docs/desktop/foo.md`.
   * Used to resolve relative Markdown links in-app instead of letting the webview navigate.
   */
  workspaceRelPath?: string;
  /** Original file name (used for extension-based detection). */
  fileName?: string;
  /**
   * Text content for text-based renderers, or base64-encoded bytes for
   * binary renderers (Image / Pdf / Office).
   */
  content: string;
  /** Language hint returned by the runtime API (e.g. "rust", "typescript"). */
  language?: string;
  /** Resolved file type — populated by `detectFileType` before dispatch. */
  fileType: FileType;
  /** File size in bytes (may be 0 when unknown). */
  size?: number;
  /** MIME type for binary files (e.g. "image/png"). */
  mimeType?: string;
  /** When true, `content` is UTF-8 HTML (sidecar preview), not base64. */
  htmlPreview?: boolean;
  /** True when binary payload was capped at `PREVIEW_MAX_BINARY_BYTES` on disk. */
  truncated?: boolean;
}

/** Every renderer receives the full PreviewState and decides what to render. */
export interface RendererProps {
  state: PreviewState;
  /** App theme — used by Markdown preview for inline Mermaid diagrams. */
  theme?: 'light' | 'dark';
  /**
   * Open another workspace file in the preview overlay (Markdown relative links).
   * If unset, relative links may still default-navigate the webview and reset the app.
   */
  onOpenWorkspaceRelativePath?: (relPath: string) => void | Promise<void>;
}
