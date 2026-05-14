// ---------------------------------------------------------------------------
// ImageRenderer — renders base64-encoded images via `<img>` data URI.
//
// Requires the Tauri CSP to include `img-src 'self' data: blob:` for clipboard/object URLs.
// Content is expected to be a base64 string (no prefix) with `mimeType`
// providing the MIME (e.g. "image/png").
// ---------------------------------------------------------------------------

import type { RendererProps } from '../types';

export function ImageRenderer({ state }: RendererProps) {
  const { content, mimeType, fileName, size, truncated } = state;

  if (!content) {
    return (
      <div className="flex h-full items-center justify-center px-6 text-center text-sm text-t-text-muted">
        图片数据为空
      </div>
    );
  }

  const src = `data:${mimeType ?? 'image/png'};base64,${content}`;

  return (
    <div className="h-full overflow-y-auto p-5 flex flex-col items-center">
      <div className="max-w-full max-h-full flex items-center justify-center bg-canvas-alt/50 rounded-lg p-4 border border-card-border">
        <img
          src={src}
          alt={fileName ?? '图片预览'}
          className="max-w-full max-h-[70vh] object-contain rounded"
        />
      </div>
      {fileName && (
        <p className="mt-3 text-xs text-t-text-muted text-center">
          {fileName}
          {size != null ? ` · ${(size / 1024).toFixed(1)} KB` : ''}
        </p>
      )}
      {truncated && (
        <p className="mt-2 text-xs text-amber-text/90 text-center max-w-md">
          预览仅包含文件前 10 MB；完整内容请使用外部程序打开。
        </p>
      )}
    </div>
  );
}
