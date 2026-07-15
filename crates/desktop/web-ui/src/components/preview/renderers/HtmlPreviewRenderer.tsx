import { useEffect, useState } from 'react';

import type { RendererProps } from '../types';
import { loadRewrittenHtmlPreviewDoc } from '../../../lib/htmlPreviewAssets';

/** Renders UTF-8 HTML (workspace pages / Office preview sidecars) in a sandboxed iframe. */
export function HtmlPreviewRenderer({ state }: RendererProps) {
  const [srcDoc, setSrcDoc] = useState(state.content);
  const [loadingAssets, setLoadingAssets] = useState(false);

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

  return (
    <div className="relative flex h-full min-h-0 w-full flex-col">
      {loadingAssets && (
        <div className="pointer-events-none absolute right-2 top-2 z-10 rounded bg-canvas-alt/90 px-2 py-0.5 text-[10px] text-t-text-muted">
          加载资源…
        </div>
      )}
      <iframe
        title={state.title}
        srcDoc={srcDoc}
        sandbox="allow-same-origin"
        className="h-full min-h-[360px] w-full flex-1 border-0 bg-white"
      />
    </div>
  );
}
