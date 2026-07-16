import { useEffect, useMemo, useState } from 'react';

import type { RendererProps } from '../types';
import { useT } from '../../../i18n';
import { loadRewrittenHtmlPreviewDoc } from '../../../lib/htmlPreviewAssets';
import {
  readHtmlPreviewAllowScriptsPref,
  writeHtmlPreviewAllowScriptsPref,
} from '../../../lib/htmlPreviewPrefs';

/** Renders UTF-8 HTML (workspace pages / Office preview sidecars) in a sandboxed iframe. */
export function HtmlPreviewRenderer({ state }: RendererProps) {
  const { t } = useT();
  const [srcDoc, setSrcDoc] = useState(state.content);
  const [loadingAssets, setLoadingAssets] = useState(false);
  const [allowScripts, setAllowScripts] = useState(() => readHtmlPreviewAllowScriptsPref());

  useEffect(() => {
    let cancelled = false;
    const html = state.content;
    const rel = state.workspaceRelPath?.trim();

    if (!rel || !html) {
      setSrcDoc(html);
      setLoadingAssets(false);
      return;
    }

    setLoadingAssets(true);
    setSrcDoc(html);

    void loadRewrittenHtmlPreviewDoc(html, {
      workspaceRelPath: rel,
      workspaceRoot: state.workspaceRoot,
      threadId: state.threadId,
      desktopHost: state.desktopHost,
    })
      .then((rewritten) => {
        if (!cancelled) {
          setSrcDoc(rewritten);
          setLoadingAssets(false);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setSrcDoc(html);
          setLoadingAssets(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [
    state.content,
    state.workspaceRelPath,
    state.workspaceRoot,
    state.threadId,
    state.desktopHost,
  ]);

  // Scripts without same-origin: page JS can run, but cannot reach parent DOM/storage.
  const sandbox = useMemo(
    () => (allowScripts ? 'allow-scripts' : 'allow-same-origin'),
    [allowScripts],
  );

  return (
    <div className="relative flex h-full min-h-0 w-full flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-border/60 px-2 py-1 text-[11px] text-t-text-muted">
        <label className="inline-flex cursor-pointer items-center gap-1.5">
          <input
            type="checkbox"
            className="accent-accent"
            checked={allowScripts}
            onChange={(e) => {
              const next = e.target.checked;
              setAllowScripts(next);
              writeHtmlPreviewAllowScriptsPref(next);
            }}
          />
          <span>{t('preview.htmlAllowScripts')}</span>
        </label>
        <span className="opacity-70">
          {allowScripts ? t('preview.htmlScriptsOnHint') : t('preview.htmlScriptsOffHint')}
        </span>
      </div>
      {loadingAssets && (
        <div className="pointer-events-none absolute right-2 top-8 z-10 rounded bg-canvas-alt/90 px-2 py-0.5 text-[10px] text-t-text-muted">
          {t('preview.htmlLoadingAssets')}
        </div>
      )}
      <iframe
        title={state.title}
        key={sandbox}
        srcDoc={srcDoc}
        sandbox={sandbox}
        className="h-full min-h-[360px] w-full flex-1 border-0 bg-white"
      />
    </div>
  );
}
