// ---------------------------------------------------------------------------
// TextRenderer — plain-text / unknown-format fallback.
// HTML-escapes the content and renders it in a <pre> block.
// ---------------------------------------------------------------------------

import type { RendererProps } from '../types';

function escapeHtml(str: string): string {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

export function TextRenderer({ state }: RendererProps) {
  const { content, fileName, size } = state;

  if (!content) {
    return (
      <div className="flex h-full items-center justify-center px-6 text-center text-sm text-t-text-muted">
        空文件
      </div>
    );
  }

  const truncated =
    content.length > 512_000 ? content.slice(0, 512_000) : content;
  const isTruncated = truncated.length < content.length;

  return (
    <div className="h-full overflow-y-auto p-5">
      <pre className="text-sm whitespace-pre-wrap font-mono leading-relaxed text-t-text break-words">
        {escapeHtml(truncated)}
      </pre>
      {isTruncated && (
        <p className="mt-2 text-xs text-amber-text/90">
          文件过大（{((size ?? content.length) / 1024).toFixed(1)} KB），仅显示前 512 KB。
          {fileName ? `（${fileName}）` : ''}
        </p>
      )}
    </div>
  );
}
