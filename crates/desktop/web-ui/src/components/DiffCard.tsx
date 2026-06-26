import { useEffect, useMemo, useRef } from 'react';
import { html } from 'diff2html';
import 'diff2html/bundles/css/diff2html.min.css';
import { useT } from '../i18n';
import { countUnifiedDiffLines } from '../lib/diff/diffEntries';
import { sanitizeHtmlForDisplay } from '../lib/sanitizeHtml';
import DiffLineStats from './diff/DiffLineStats';

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
  const lineStats = useMemo(() => countUnifiedDiffLines(diffText), [diffText]);

  useEffect(() => {
    if (!containerRef.current) return;

    try {
      const diffHtml = html(diffText, {
        drawFileList: false,
        matching: 'lines',
        outputFormat,
        renderNothingWhenEmpty: false,
      });

      containerRef.current.innerHTML = `<div class="d2h-wrapper">${sanitizeHtmlForDisplay(diffHtml)}</div>`;
    } catch {
      if (containerRef.current) {
        containerRef.current.innerHTML = `<pre class="text-[11px] whitespace-pre-wrap text-t-text-secondary">${escapeHtml(diffText)}</pre>`;
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
      role="region"
      aria-label={t('a11y.diffRegion', { fileName: fileName ?? 'diff' })}
    >
      {fileName && (
        <div className="flex shrink-0 items-center gap-2 border-b border-divider bg-canvas-alt px-3 py-1.5">
          <span className="min-w-0 flex-1 truncate text-[10px] font-mono text-t-text-muted">
            {fileName}
          </span>
          <DiffLineStats added={lineStats.added} removed={lineStats.removed} />
          {onOpenInPanel && (
            <button
              type="button"
              className="shrink-0 rounded px-1.5 py-0.5 text-[10px] text-accent hover:bg-hover"
              onClick={onOpenInPanel}
              aria-label={t('a11y.openDiffInPanel', { fileName: fileName ?? 'diff' })}
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
