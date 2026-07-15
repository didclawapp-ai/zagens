import { useState } from 'react';

import type { RendererProps } from '../types';
import { CodeRenderer } from './CodeRenderer';
import { HtmlPreviewRenderer } from './HtmlPreviewRenderer';

type HtmlViewMode = 'code' | 'preview';

/** P0a: workspace HTML defaults to visual preview; users can switch to「代码」. */
function defaultHtmlViewMode(): HtmlViewMode {
  return 'preview';
}

function tabClass(active: boolean): string {
  return [
    'rounded-md px-2.5 py-1 text-xs font-medium transition-colors',
    active
      ? 'bg-accent/15 text-accent'
      : 'text-t-text-muted hover:bg-hover hover:text-t-text',
  ].join(' ');
}

/** HTML workspace files — toggle between syntax-highlighted source and iframe preview. */
export function HtmlRenderer({ state }: RendererProps) {
  const [mode, setMode] = useState<HtmlViewMode>(() => defaultHtmlViewMode());

  const codeState =
    state.language?.trim()
      ? state
      : { ...state, language: 'html' };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="shrink-0 flex items-center gap-1 border-b border-divider bg-canvas-alt/30 px-3 py-1.5">
        <button
          type="button"
          className={tabClass(mode === 'code')}
          onClick={() => setMode('code')}
        >
          代码
        </button>
        <button
          type="button"
          className={tabClass(mode === 'preview')}
          onClick={() => setMode('preview')}
        >
          预览
        </button>
      </div>
      <div className="flex-1 min-h-0 overflow-hidden">
        {mode === 'code' ? (
          <CodeRenderer state={codeState} />
        ) : (
          <HtmlPreviewRenderer state={state} />
        )}
      </div>
    </div>
  );
}
