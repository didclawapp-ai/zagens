import { useEffect, useRef } from 'react';
import { html } from 'diff2html';
import 'diff2html/bundles/css/diff2html.min.css';

interface Props {
  diffText: string;
  fileName?: string;
  /** 'side-by-side' or 'line-by-line' */
  outputFormat?: 'side-by-side' | 'line-by-line';
}

export default function DiffCard({ diffText, fileName, outputFormat = 'side-by-side' }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!containerRef.current) return;

    // Try to render; fall back to plain <pre> if parsing fails
    try {
      const diffHtml = html(diffText, {
        drawFileList: false,
        matching: 'lines',
        outputFormat,
        renderNothingWhenEmpty: false,
      });

      // Wrap in a scoped container so diff2html styles don't collide
      containerRef.current.innerHTML = `<div class="d2h-wrapper" style="font-size:11px;">${diffHtml}</div>`;

      // Override some diff2html colors for dark theme
      const style = document.createElement('style');
      style.textContent = `
        .d2h-wrapper .d2h-file-header { background: var(--hover); border-color: var(--divider); }
        .d2h-wrapper .d2h-code-line { font-size: 11px; }
        .d2h-wrapper .d2h-ins { background: rgba(16,185,129,0.08) !important; }
        .d2h-wrapper .d2h-del { background: rgba(239,68,68,0.08) !important; }
        .d2h-wrapper .d2h-code-line-ctn { color: var(--text-secondary); }
        .d2h-wrapper .d2h-file-diff { border-color: var(--divider); }
      `;
      containerRef.current.appendChild(style);
    } catch {
      if (containerRef.current) {
        containerRef.current.innerHTML = `<pre style="font-size:11px;white-space:pre-wrap;color:var(--text-secondary);">${escapeHtml(diffText)}</pre>`;
      }
    }
  }, [diffText, outputFormat]);

  return (
    <div className="rounded-lg border border-card-border overflow-hidden my-2">
      {fileName && (
        <div className="flex items-center px-3 py-1.5 bg-canvas-alt border-b border-divider">
          <span className="text-[10px] text-t-text-muted font-mono">📝 {fileName}</span>
        </div>
      )}
      <div ref={containerRef} className="max-h-[60vh] overflow-auto" />
    </div>
  );
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}
