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
  Office = 'office',
  Unknown = 'unknown',
}

export interface PreviewState {
  /** Breadcrumb / display name shown in the overlay header. */
  title: string;
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
  /** True when binary payload was capped at `PREVIEW_MAX_BINARY_BYTES` on disk. */
  truncated?: boolean;
}

/** Every renderer receives the full PreviewState and decides what to render. */
export interface RendererProps {
  state: PreviewState;
}
