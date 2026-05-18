import { useEffect, useRef } from 'react';
import { html } from 'diff2html';
import 'diff2html/bundles/css/diff2html.min.css';
import { useT } from '../i18n';

interface Props {
  diffText: string;
  fileName?: string;
  /** 'side-by-side' or 'line-by-line' */
  outputFormat?: 'side-by-side' | 'line-by-line';
  /** `panel` fills the right workspace tab; `inline` is for chat tool cards */
  variant?: 'inline' | 'panel';
  onOpenInPanel?: () => void;
}

export default function DiffCard({
  diffText,
  fileName,
  outputFormat = 'side-by-side',
  variant = 'inline',
  onOpenInPanel,
}: Props) {
  const { t } = useT();
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!containerRef.current) return;

    try {
      const diffHtml = html(diffText, {
        drawFileList: false,
        matching: 'lines',
        outputFormat,
        renderNothingWhenEmpty: false,
      });

      containerRef.current.innerHTML = `<div class="d2h-wrapper" style="font-size:11px;">${diffHtml}</div>`;

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

  const isPanel = variant === 'panel';

  return (
    <div
      className={
        isPanel
          ? 'flex h-full min-h-0 flex-col overflow-hidden rounded-lg border border-card-border'
          : 'my-2 overflow-hidden rounded-lg border border-card-border'
      }
    >
      {fileName && (
        <div className="flex shrink-0 items-center gap-2 border-b border-divider bg-canvas-alt px-3 py-1.5">
          <span className="min-w-0 flex-1 truncate text-[10px] font-mono text-t-text-muted">
            {fileName}
          </span>
          {onOpenInPanel && (
            <button
              type="button"
              className="shrink-0 rounded px-1.5 py-0.5 text-[10px] text-accent hover:bg-hover"
              onClick={onOpenInPanel}
            >
              {t('diff.openInPanel')}
            </button>
          )}
        </div>
      )}
      <div
        ref={containerRef}
        className={isPanel ? 'min-h-0 flex-1 overflow-auto' : 'max-h-[60vh] overflow-auto'}
      />
    </div>
  );
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}
