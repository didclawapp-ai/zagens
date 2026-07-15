/**
 * Browser pane UI (P1 spike) — toolbar in RightPanel; content is a child WebView
 * (embedded) or a separate Browser window (windowed).
 */

import { invoke } from '@tauri-apps/api/core';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useT } from '../../i18n';

type BrowserMode = 'embedded' | 'windowed';

type BrowserState = {
  parentLabel: string;
  hostLabel: string;
  mode: BrowserMode;
  url: string;
  title: string;
  visible: boolean;
};

type BrowserSnapshot = {
  url: string;
  title: string;
  text: string;
  nodes: Array<{ ref: string; role: string; name: string }>;
};

type BrowserError = {
  code?: string;
  message?: string;
  hint?: string;
};

function errMessage(e: unknown): string {
  if (e && typeof e === 'object') {
    const o = e as BrowserError & { error?: string };
    return o.message || o.hint || o.error || JSON.stringify(e);
  }
  return String(e);
}

export default function BrowserPane({ desktopHost }: { desktopHost: boolean }) {
  const { t } = useT();
  const hostRef = useRef<HTMLDivElement>(null);
  const [urlInput, setUrlInput] = useState('http://127.0.0.1:5173/');
  const [state, setState] = useState<BrowserState | null>(null);
  const [status, setStatus] = useState<string>('');
  const [snapshotPreview, setSnapshotPreview] = useState<string>('');
  const [busy, setBusy] = useState(false);

  const syncBounds = useCallback(async (visible: boolean) => {
    const el = hostRef.current;
    if (!el || !state || state.mode !== 'embedded') return;
    const r = el.getBoundingClientRect();
    try {
      await invoke('browser_set_bounds', {
        args: {
          x: r.left,
          y: r.top,
          width: Math.max(1, r.width),
          height: Math.max(1, r.height),
          visible,
        },
      });
    } catch {
      /* host may be missing while recreating */
    }
  }, [state]);

  const createHost = useCallback(
    async (mode: 'auto' | 'embedded' | 'windowed' = 'auto') => {
      if (!desktopHost) {
        setStatus(t('browser.needDesktop'));
        return;
      }
      setBusy(true);
      setStatus('');
      try {
        const el = hostRef.current;
        const r = el?.getBoundingClientRect();
        const st = await invoke<BrowserState>('browser_create', {
          args: {
            mode,
            url: urlInput.trim() || 'about:blank',
            x: r?.left ?? 0,
            y: r?.top ?? 0,
            width: Math.max(1, r?.width ?? 400),
            height: Math.max(1, r?.height ?? 600),
          },
        });
        setState(st);
        setStatus(
          st.mode === 'embedded'
            ? t('browser.modeEmbedded')
            : t('browser.modeWindowed'),
        );
      } catch (e) {
        setStatus(errMessage(e));
      } finally {
        setBusy(false);
      }
    },
    [desktopHost, t, urlInput],
  );

  const navigate = useCallback(async () => {
    if (!desktopHost) return;
    setBusy(true);
    try {
      if (!state) {
        await createHost('auto');
        return;
      }
      const st = await invoke<BrowserState>('browser_navigate', {
        args: { url: urlInput.trim(), actor: 'human' },
      });
      setState(st);
      setUrlInput(st.url || urlInput);
    } catch (e) {
      setStatus(errMessage(e));
    } finally {
      setBusy(false);
    }
  }, [createHost, desktopHost, state, urlInput]);

  const takeSnapshot = useCallback(async () => {
    setBusy(true);
    try {
      const snap = await invoke<BrowserSnapshot>('browser_snapshot');
      const head = `# ${snap.title || '(no title)'}\n${snap.url}\n\n`;
      const nodes = snap.nodes
        .slice(0, 20)
        .map((n) => `- [${n.ref}] ${n.role}: ${n.name}`)
        .join('\n');
      setSnapshotPreview(`${head}${snap.text.slice(0, 2000)}\n\n## nodes\n${nodes}`);
    } catch (e) {
      setStatus(errMessage(e));
    } finally {
      setBusy(false);
    }
  }, []);

  const destroy = useCallback(async () => {
    try {
      await invoke('browser_destroy');
    } catch {
      /* ignore */
    }
    setState(null);
    setSnapshotPreview('');
    setStatus('');
  }, []);

  // Create on mount; destroy / hide on unmount.
  useEffect(() => {
    if (!desktopHost) return;
    void createHost('auto');
    return () => {
      void invoke('browser_set_bounds', {
        args: { x: 0, y: 0, width: 1, height: 1, visible: false },
      }).catch(() => undefined);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- mount once per panel open
  }, [desktopHost]);

  useEffect(() => {
    if (!state || state.mode !== 'embedded') return;
    const el = hostRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      void syncBounds(true);
    });
    ro.observe(el);
    void syncBounds(true);
    const onResize = () => {
      void syncBounds(true);
    };
    window.addEventListener('resize', onResize);
    return () => {
      ro.disconnect();
      window.removeEventListener('resize', onResize);
    };
  }, [state, syncBounds]);

  if (!desktopHost) {
    return (
      <p className="p-4 text-sm text-t-text-muted">{t('browser.needDesktop')}</p>
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="shrink-0 space-y-2 border-b border-divider px-3 py-2">
        <div className="flex items-center gap-1">
          <input
            type="text"
            className="min-w-0 flex-1 rounded-md border border-divider bg-canvas px-2 py-1 text-xs text-t-text"
            value={urlInput}
            onChange={(e) => setUrlInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void navigate();
            }}
            placeholder="http://127.0.0.1:PORT/"
            aria-label={t('browser.urlAria')}
          />
          <button
            type="button"
            className="rounded-md bg-accent/15 px-2 py-1 text-xs font-medium text-accent disabled:opacity-50"
            disabled={busy}
            onClick={() => void navigate()}
          >
            {t('browser.go')}
          </button>
        </div>
        <div className="flex flex-wrap items-center gap-1">
          <button
            type="button"
            className="rounded-md px-2 py-1 text-[11px] text-t-text-muted hover:bg-hover"
            disabled={busy}
            onClick={() => void createHost('embedded')}
          >
            {t('browser.tryEmbedded')}
          </button>
          <button
            type="button"
            className="rounded-md px-2 py-1 text-[11px] text-t-text-muted hover:bg-hover"
            disabled={busy}
            onClick={() => void createHost('windowed')}
          >
            {t('browser.tryWindowed')}
          </button>
          <button
            type="button"
            className="rounded-md px-2 py-1 text-[11px] text-t-text-muted hover:bg-hover"
            disabled={busy || !state}
            onClick={() => void takeSnapshot()}
          >
            {t('browser.snapshot')}
          </button>
          <button
            type="button"
            className="rounded-md px-2 py-1 text-[11px] text-t-text-muted hover:bg-hover"
            disabled={busy}
            onClick={() => void invoke('browser_focus_content').catch(() => undefined)}
          >
            {t('browser.focus')}
          </button>
          <button
            type="button"
            className="rounded-md px-2 py-1 text-[11px] text-t-text-muted hover:bg-hover"
            disabled={busy}
            onClick={() => void destroy()}
          >
            {t('browser.destroy')}
          </button>
        </div>
        {status ? (
          <p className="text-[11px] text-t-text-muted" role="status">
            {status}
            {state ? ` · ${state.hostLabel}` : ''}
          </p>
        ) : null}
      </div>

      <div
        ref={hostRef}
        data-browser-host
        className="relative min-h-[240px] flex-1 bg-canvas-alt/40"
      >
        {state?.mode === 'windowed' ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 p-4 text-center text-sm text-t-text-muted">
            <p>{t('browser.windowedHint')}</p>
            <button
              type="button"
              className="rounded-md bg-accent/15 px-3 py-1.5 text-xs font-medium text-accent"
              onClick={() => void invoke('browser_focus_content')}
            >
              {t('browser.focus')}
            </button>
          </div>
        ) : (
          <div className="pointer-events-none absolute inset-0 flex items-center justify-center text-xs text-t-text-muted/80">
            {state?.mode === 'embedded' ? t('browser.embeddedPlaceholder') : t('browser.loading')}
          </div>
        )}
      </div>

      {snapshotPreview ? (
        <pre className="max-h-40 shrink-0 overflow-auto border-t border-divider bg-canvas-alt/50 p-2 text-[10px] text-t-text-muted whitespace-pre-wrap">
          {snapshotPreview}
        </pre>
      ) : null}
    </div>
  );
}
