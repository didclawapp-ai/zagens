/**
 * Browser pane UI (P1) — toolbar in RightPanel; content is a child WebView
 * (embedded) or a separate Browser window (windowed).
 */

import { invoke } from '@tauri-apps/api/core';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useT } from '../../i18n';
import { listenCurrentWebviewEvent } from '../../lib/tauriListen';
import {
  readAllowPrivateLanPref,
  readBrowserModePref,
  readBrowserYoloPref,
  readDestroyOnClosePref,
  readPersistProfilePref,
  writeAllowPrivateLanPref,
  writeBrowserModePref,
  writeBrowserYoloPref,
  writeDestroyOnClosePref,
  writePersistProfilePref,
  type BrowserEmbedMode,
} from '../../lib/browser/browserPrefs';

type BrowserMode = 'embedded' | 'windowed';

type BrowserState = {
  parentLabel: string;
  hostLabel: string;
  mode: BrowserMode;
  url: string;
  title: string;
  visible: boolean;
  loading: boolean;
  canGoBack: boolean;
  canGoForward: boolean;
  security: string;
  persistProfile: boolean;
};

type BrowserSnapshot = {
  url: string;
  title: string;
  text: string;
  nodes: Array<{ ref: string; role: string; name: string }>;
  screenshotNote?: string | null;
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

function securityBadgeClass(security: string): string {
  if (security === 'loopback') return 'bg-emerald-500/15 text-emerald-700 dark:text-emerald-300';
  if (security === 'external') return 'bg-amber-500/15 text-amber-800 dark:text-amber-200';
  if (security === 'file') return 'bg-sky-500/15 text-sky-800 dark:text-sky-200';
  return 'bg-hover text-t-text-muted';
}

function securityLabel(
  t: (key: string, params?: Record<string, string>) => string,
  security: string,
): string {
  switch (security) {
    case 'loopback':
      return t('browser.security.loopback');
    case 'external':
      return t('browser.security.external');
    case 'file':
      return t('browser.security.file');
    case 'blank':
      return t('browser.security.blank');
    default:
      return t('browser.security.unknown');
  }
}

const btnClass =
  'rounded-md px-2 py-1 text-[11px] text-t-text-muted hover:bg-hover disabled:opacity-40';
const btnPrimary =
  'rounded-md bg-accent/15 px-2 py-1 text-xs font-medium text-accent disabled:opacity-50';

export default function BrowserPane({ desktopHost }: { desktopHost: boolean }) {
  const { t } = useT();
  const hostRef = useRef<HTMLDivElement>(null);
  const [urlInput, setUrlInput] = useState('http://127.0.0.1:5173/');
  const [state, setState] = useState<BrowserState | null>(null);
  const [status, setStatus] = useState<string>('');
  const [snapshotPreview, setSnapshotPreview] = useState<string>('');
  const [busy, setBusy] = useState(false);
  const [embedMode, setEmbedMode] = useState<BrowserEmbedMode>(() => readBrowserModePref());
  const [persistProfile, setPersistProfile] = useState(() => readPersistProfilePref());
  const [destroyOnClose, setDestroyOnClose] = useState(() => readDestroyOnClosePref());
  const [allowLan, setAllowLan] = useState(() => readAllowPrivateLanPref());
  const [browserYolo, setBrowserYolo] = useState(() => readBrowserYoloPref());
  const destroyOnCloseRef = useRef(destroyOnClose);
  destroyOnCloseRef.current = destroyOnClose;

  const applyState = useCallback((st: BrowserState) => {
    setState(st);
    if (st.url) setUrlInput(st.url);
  }, []);

  const syncBounds = useCallback(
    async (visible: boolean) => {
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
    },
    [state],
  );

  const createHost = useCallback(
    async (mode: BrowserEmbedMode = embedMode) => {
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
            persistProfile,
          },
        });
        applyState(st);
        setStatus(
          st.mode === 'embedded' ? t('browser.modeEmbedded') : t('browser.modeWindowed'),
        );
      } catch (e) {
        setStatus(errMessage(e));
      } finally {
        setBusy(false);
      }
    },
    [applyState, desktopHost, embedMode, persistProfile, t, urlInput],
  );

  const navigate = useCallback(async () => {
    if (!desktopHost) return;
    setBusy(true);
    try {
      if (!state) {
        await createHost(embedMode);
        return;
      }
      const st = await invoke<BrowserState>('browser_navigate', {
        args: { url: urlInput.trim(), actor: 'human' },
      });
      applyState(st);
    } catch (e) {
      setStatus(errMessage(e));
    } finally {
      setBusy(false);
    }
  }, [applyState, createHost, desktopHost, embedMode, state, urlInput]);

  const goBack = useCallback(async () => {
    setBusy(true);
    try {
      const st = await invoke<BrowserState>('browser_back');
      applyState(st);
    } catch (e) {
      setStatus(errMessage(e));
    } finally {
      setBusy(false);
    }
  }, [applyState]);

  const goForward = useCallback(async () => {
    setBusy(true);
    try {
      const st = await invoke<BrowserState>('browser_forward');
      applyState(st);
    } catch (e) {
      setStatus(errMessage(e));
    } finally {
      setBusy(false);
    }
  }, [applyState]);

  const reload = useCallback(async () => {
    setBusy(true);
    try {
      const st = await invoke<BrowserState>('browser_reload');
      applyState(st);
    } catch (e) {
      setStatus(errMessage(e));
    } finally {
      setBusy(false);
    }
  }, [applyState]);

  const takeSnapshot = useCallback(async () => {
    setBusy(true);
    try {
      const snap = await invoke<BrowserSnapshot>('browser_snapshot', {
        args: { includeScreenshot: false },
      });
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

  const allowCurrentHost = useCallback(async () => {
    try {
      const prefs = await invoke<{ allowlist: string[] }>('browser_allow_host', {
        args: {},
      });
      setStatus(
        t('browser.allowHostDone', {
          list: prefs.allowlist.join(', ') || '—',
        }),
      );
    } catch (e) {
      setStatus(errMessage(e));
    }
  }, [t]);

  const onPersistToggle = useCallback(
    async (next: boolean) => {
      setPersistProfile(next);
      writePersistProfilePref(next);
      try {
        await invoke('browser_set_persist_profile', { args: { persist: next } });
        setStatus(next ? t('browser.persistOn') : t('browser.persistOff'));
      } catch (e) {
        setStatus(errMessage(e));
      }
    },
    [t],
  );

  const onAllowLanToggle = useCallback(
    async (next: boolean) => {
      setAllowLan(next);
      writeAllowPrivateLanPref(next);
      try {
        await invoke('browser_set_prefs', {
          args: { allowPrivateLan: next },
        });
      } catch (e) {
        setStatus(errMessage(e));
      }
    },
    [],
  );

  const onBrowserYoloToggle = useCallback(
    async (next: boolean) => {
      setBrowserYolo(next);
      writeBrowserYoloPref(next);
      try {
        await invoke('browser_set_prefs', { args: { yolo: next } });
        setStatus(next ? t('browser.yoloOn') : t('browser.yoloOff'));
      } catch (e) {
        setStatus(errMessage(e));
      }
    },
    [t],
  );

  const startPreview = useCallback(async () => {
    setBusy(true);
    try {
      const res = await invoke<{
        ready: boolean;
        note: string;
        browser?: BrowserState;
      }>('browser_preview_start');
      if (res.browser) applyState(res.browser);
      setStatus(res.note || (res.ready ? t('browser.previewReady') : t('browser.previewStarted')));
    } catch (e) {
      setStatus(errMessage(e));
    } finally {
      setBusy(false);
    }
  }, [applyState, t]);

  // Sync shell prefs once; create host on mount.
  useEffect(() => {
    if (!desktopHost) return;
    let cancelled = false;
    void (async () => {
      try {
        await invoke('browser_set_prefs', {
          args: {
            persistProfile,
            allowPrivateLan: allowLan,
            yolo: browserYolo,
          },
        });
      } catch {
        /* ignore */
      }
      if (!cancelled) await createHost(embedMode);
    })();
    return () => {
      cancelled = true;
      if (destroyOnCloseRef.current) {
        void invoke('browser_destroy').catch(() => undefined);
      } else {
        void invoke('browser_set_bounds', {
          args: { x: 0, y: 0, width: 1, height: 1, visible: false },
        }).catch(() => undefined);
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- mount once per panel open
  }, [desktopHost]);

  useEffect(() => {
    if (!desktopHost) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void listenCurrentWebviewEvent<BrowserState>(
      'browser://state',
      (payload) => {
        if (!cancelled) applyState(payload);
      },
      { cancelled: () => cancelled },
    ).then((fn) => {
      unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [applyState, desktopHost]);

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

  const security = state?.security ?? 'blank';

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="shrink-0 space-y-2 border-b border-divider px-3 py-2">
        <div className="flex items-center gap-1">
          <button
            type="button"
            className={btnClass}
            disabled={busy || !state?.canGoBack}
            onClick={() => void goBack()}
            aria-label={t('browser.back')}
            title={t('browser.back')}
          >
            ←
          </button>
          <button
            type="button"
            className={btnClass}
            disabled={busy || !state?.canGoForward}
            onClick={() => void goForward()}
            aria-label={t('browser.forward')}
            title={t('browser.forward')}
          >
            →
          </button>
          <button
            type="button"
            className={btnClass}
            disabled={busy || !state}
            onClick={() => void reload()}
            aria-label={t('browser.reload')}
            title={t('browser.reload')}
          >
            ↻
          </button>
          <span
            className={`shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium ${securityBadgeClass(security)}`}
            title={securityLabel(t, security)}
          >
            {securityLabel(t, security)}
          </span>
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
            className={btnPrimary}
            disabled={busy}
            onClick={() => void navigate()}
          >
            {t('browser.go')}
          </button>
        </div>

        <div className="flex flex-wrap items-center gap-1">
          <label className="flex items-center gap-1 text-[11px] text-t-text-muted">
            <span>{t('browser.modeLabel')}</span>
            <select
              className="rounded border border-divider bg-canvas px-1 py-0.5 text-[11px]"
              value={embedMode}
              onChange={(e) => {
                const next = e.target.value as BrowserEmbedMode;
                setEmbedMode(next);
                writeBrowserModePref(next);
              }}
            >
              <option value="auto">{t('browser.modeAuto')}</option>
              <option value="embedded">{t('browser.modeEmbeddedShort')}</option>
              <option value="windowed">{t('browser.modeWindowedShort')}</option>
            </select>
          </label>
          <button
            type="button"
            className={btnClass}
            disabled={busy}
            onClick={() => void createHost(embedMode)}
          >
            {t('browser.recreate')}
          </button>
          <button
            type="button"
            className={btnClass}
            disabled={busy}
            onClick={() => void startPreview()}
          >
            {t('browser.startPreview')}
          </button>
          <button
            type="button"
            className={btnClass}
            disabled={busy || !state}
            onClick={() => void takeSnapshot()}
          >
            {t('browser.snapshot')}
          </button>
          <button
            type="button"
            className={btnClass}
            disabled={busy || !state}
            onClick={() => void invoke('browser_focus_content').catch(() => undefined)}
          >
            {t('browser.focus')}
          </button>
          <button
            type="button"
            className={btnClass}
            disabled={busy || !state || security !== 'external'}
            onClick={() => void allowCurrentHost()}
          >
            {t('browser.allowHost')}
          </button>
          <button
            type="button"
            className={btnClass}
            disabled={busy}
            onClick={() => void destroy()}
          >
            {t('browser.destroy')}
          </button>
        </div>

        <div className="flex flex-wrap items-center gap-3 text-[11px] text-t-text-muted">
          <label className="flex items-center gap-1">
            <input
              type="checkbox"
              checked={persistProfile}
              onChange={(e) => void onPersistToggle(e.target.checked)}
            />
            {t('browser.persistProfile')}
          </label>
          <label className="flex items-center gap-1">
            <input
              type="checkbox"
              checked={destroyOnClose}
              onChange={(e) => {
                setDestroyOnClose(e.target.checked);
                writeDestroyOnClosePref(e.target.checked);
              }}
            />
            {t('browser.destroyOnClose')}
          </label>
          <label className="flex items-center gap-1">
            <input
              type="checkbox"
              checked={allowLan}
              onChange={(e) => void onAllowLanToggle(e.target.checked)}
            />
            {t('browser.allowPrivateLan')}
          </label>
          <label className="flex items-center gap-1" title={t('browser.yoloHint')}>
            <input
              type="checkbox"
              checked={browserYolo}
              onChange={(e) => void onBrowserYoloToggle(e.target.checked)}
            />
            {t('browser.browserYolo')}
          </label>
          {state?.loading ? (
            <span className="text-accent" role="status">
              {t('browser.pageLoading')}
            </span>
          ) : null}
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
              className={btnPrimary}
              onClick={() => void invoke('browser_focus_content')}
            >
              {t('browser.focus')}
            </button>
          </div>
        ) : (
          <div className="pointer-events-none absolute inset-0 flex items-center justify-center text-xs text-t-text-muted/80">
            {state?.mode === 'embedded'
              ? t('browser.embeddedPlaceholder')
              : t('browser.loading')}
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
