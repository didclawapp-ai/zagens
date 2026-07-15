import { useCallback, useEffect, useId, useMemo, useRef, useState } from 'react';
import { useT } from '../../i18n';
import type { TranslationKey } from '../../i18n/keys';
import type { Theme } from '../../lib/appPreferences';
import { handleTabListKeyDown } from '../../lib/a11y/rovingTabList';
import { subscribeCurrentWebviewEvent } from '../../lib/tauriListen';
import {
  killTerminal,
  spawnTerminal,
  writeTerminal,
  type TerminalDataEvent,
  type TerminalExitEvent,
  type TerminalShellKind,
} from '../../lib/terminal/ptyApi';
import {
  buildCdCommand,
  readCopyOnSelectPref,
  readFontSizePref,
  TERMINAL_FONT_SIZES,
  writeCopyOnSelectPref,
  writeFontSizePref,
  type TerminalFontSize,
} from '../../lib/terminal/terminalPrefs';
import { integratedTerminalChrome, integratedTerminalThemeForApp } from '../../lib/terminal/xtermTheme';
import InteractiveTerminalView, { type TerminalViewActions } from './InteractiveTerminalView';

/** Must match MAX_SESSIONS_PER_WINDOW in crates/desktop/src/terminal.rs */
const MAX_SESSIONS = 4;
const DEFAULT_COLS = 80;
const DEFAULT_ROWS = 24;
const SHELL_PREF_KEY = 'zagens-desktop-terminal-shell';
const PROFILE_PREF_KEY = 'zagens-desktop-terminal-load-profile';

const WINDOWS_SHELLS: TerminalShellKind[] = ['default', 'pwsh', 'powershell', 'cmd'];
const UNIX_SHELLS: TerminalShellKind[] = ['default', 'bash', 'zsh', 'sh'];

export interface TerminalSessionMeta {
  id: string;
  title: string;
  /** Workspace path used when this PTY was spawned. */
  cwd: string;
  shell: TerminalShellKind;
  exited?: boolean;
  exitCode?: number | null;
}

interface Props {
  workspaceRoot: string;
  desktopHost: boolean;
  /** OS platform string from Tauri (`windows` / `macos` / `linux`). */
  platform: string;
  /** App light/dark/dusk theme — drives integrated terminal chrome + xterm palette. */
  theme: Theme;
  /** Panel tab is visible — spawn / fit when true */
  active: boolean;
  /** Parent bump requests an additional PTY session (Ctrl+Shift+`). */
  createSessionNonce?: number;
  /** Parent bump requests `cd` into `cdRequestPath` in a live session. */
  cdRequestNonce?: number;
  cdRequestPath?: string | null;
}

function isWindowsPlatform(platform: string): boolean {
  return platform.toLowerCase().includes('win');
}

function normalizeWorkspacePath(path: string, isWindows: boolean): string {
  const trimmed = path.trim().replace(/[/\\]+$/, '');
  const unified = trimmed.replace(/\\/g, '/');
  return isWindows ? unified.toLowerCase() : unified;
}

function pathsEqual(a: string, b: string, isWindows: boolean): boolean {
  return normalizeWorkspacePath(a, isWindows) === normalizeWorkspacePath(b, isWindows);
}

function readShellPref(isWindows: boolean): TerminalShellKind {
  try {
    const raw = localStorage.getItem(SHELL_PREF_KEY);
    const allowed = isWindows ? WINDOWS_SHELLS : UNIX_SHELLS;
    if (raw && (allowed as string[]).includes(raw)) {
      return raw as TerminalShellKind;
    }
  } catch {
    /* ignore */
  }
  return 'default';
}

function readProfilePref(): boolean {
  try {
    return localStorage.getItem(PROFILE_PREF_KEY) === '1';
  } catch {
    return false;
  }
}

function nextTerminalTitle(existing: TerminalSessionMeta[]): string {
  const used = new Set(existing.map((s) => s.title));
  let n = existing.length + 1;
  while (used.has(`Terminal ${n}`)) {
    n += 1;
  }
  return `Terminal ${n}`;
}

function exitStatusLine(
  t: (key: string, vars?: Record<string, string>) => string,
  code: number | null,
): string {
  if (code != null && code !== 0) {
    return `\r\n\x1b[90m${t('terminalInteractive.processExited', { code: String(code) })}\x1b[0m\r\n`;
  }
  return `\r\n\x1b[90m${t('terminalInteractive.processEnded')}\x1b[0m\r\n`;
}

function shellLabelKey(shell: TerminalShellKind): TranslationKey {
  switch (shell) {
    case 'default':
      return 'terminal.shellDefault';
    case 'pwsh':
      return 'terminal.shellPwsh';
    case 'powershell':
      return 'terminal.shellPowershell';
    case 'cmd':
      return 'terminal.shellCmd';
    case 'bash':
      return 'terminal.shellBash';
    case 'zsh':
      return 'terminal.shellZsh';
    case 'sh':
      return 'terminal.shellSh';
  }
}

function shortPath(path: string): string {
  const trimmed = path.trim();
  if (trimmed.length <= 48) return trimmed;
  return `…${trimmed.slice(-46)}`;
}

export default function TerminalPanel({
  workspaceRoot,
  desktopHost,
  platform,
  theme,
  active,
  createSessionNonce = 0,
  cdRequestNonce = 0,
  cdRequestPath = null,
}: Props) {
  const { t } = useT();
  const menuId = useId();
  const searchInputId = useId();
  const isWindows = isWindowsPlatform(platform);
  const shellOptions = isWindows ? WINDOWS_SHELLS : UNIX_SHELLS;
  const chrome = useMemo(() => integratedTerminalChrome(theme), [theme]);
  const xtermTheme = useMemo(() => integratedTerminalThemeForApp(theme), [theme]);
  const [sessions, setSessions] = useState<TerminalSessionMeta[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [spawning, setSpawning] = useState(false);
  const [shellKind, setShellKind] = useState<TerminalShellKind>(() => readShellPref(isWindows));
  const [loadProfile, setLoadProfile] = useState(() => readProfilePref());
  const [fontSize, setFontSize] = useState<TerminalFontSize>(() => readFontSizePref());
  const [copyOnSelect, setCopyOnSelect] = useState(() => readCopyOnSelectPref());
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');
  const [cwdBannerDismissed, setCwdBannerDismissed] = useState(false);
  const buffersRef = useRef<Map<string, string>>(new Map());
  const writersRef = useRef<Map<string, (chunk: string) => void>>(new Map());
  const actionsRef = useRef<Map<string, TerminalViewActions>>(new Map());
  const sessionsRef = useRef(sessions);
  sessionsRef.current = sessions;
  const spawnLockRef = useRef(false);
  const handledCreateNonceRef = useRef(0);
  const handledCdNonceRef = useRef(0);
  const menuRef = useRef<HTMLDivElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const workspaceRef = useRef(workspaceRoot);
  workspaceRef.current = workspaceRoot;
  const shellKindRef = useRef(shellKind);
  shellKindRef.current = shellKind;
  const loadProfileRef = useRef(loadProfile);
  loadProfileRef.current = loadProfile;
  const tRef = useRef(t);
  tRef.current = t;

  const showProfileToggle =
    isWindows && (shellKind === 'default' || shellKind === 'pwsh' || shellKind === 'powershell');

  const activeSession = sessions.find((s) => s.id === activeId) ?? sessions[0] ?? null;
  const sessionIds = useMemo(() => sessions.map((s) => s.id), [sessions]);
  const terminalTabId = (sessionId: string) => `terminal-tab-${sessionId}`;
  const terminalTabPanelId = 'terminal-tabpanel';

  const cwdStale = useMemo(() => {
    if (sessions.length === 0 || !workspaceRoot.trim()) return false;
    return sessions.some((s) => !pathsEqual(s.cwd, workspaceRoot, isWindows));
  }, [sessions, workspaceRoot, isWindows]);

  const showCwdBanner = cwdStale && !cwdBannerDismissed;

  // Reset dismiss when workspace changes again or stale resolves.
  useEffect(() => {
    setCwdBannerDismissed(false);
  }, [workspaceRoot]);

  const onSessionTabListKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLDivElement>) => {
      if (!activeId) {
        return;
      }
      handleTabListKeyDown(e, sessionIds, activeId, setActiveId, terminalTabId);
    },
    [sessionIds, activeId],
  );

  const appendOutput = useCallback((sessionId: string, chunk: string) => {
    const prev = buffersRef.current.get(sessionId) ?? '';
    buffersRef.current.set(sessionId, prev + chunk);
  }, []);

  const registerWriter = useCallback((sessionId: string, write: (chunk: string) => void) => {
    writersRef.current.set(sessionId, write);
    return () => {
      if (writersRef.current.get(sessionId) === write) {
        writersRef.current.delete(sessionId);
      }
    };
  }, []);

  const registerActions = useCallback((sessionId: string, actions: TerminalViewActions) => {
    actionsRef.current.set(sessionId, actions);
    return () => {
      if (actionsRef.current.get(sessionId) === actions) {
        actionsRef.current.delete(sessionId);
      }
    };
  }, []);

  const onShellChange = useCallback((next: TerminalShellKind) => {
    setShellKind(next);
    try {
      localStorage.setItem(SHELL_PREF_KEY, next);
    } catch {
      /* ignore */
    }
  }, []);

  const onLoadProfileChange = useCallback((next: boolean) => {
    setLoadProfile(next);
    try {
      localStorage.setItem(PROFILE_PREF_KEY, next ? '1' : '0');
    } catch {
      /* ignore */
    }
  }, []);

  const onFontSizeChange = useCallback((next: TerminalFontSize) => {
    setFontSize(next);
    writeFontSizePref(next);
  }, []);

  const onCopyOnSelectChange = useCallback((next: boolean) => {
    setCopyOnSelect(next);
    writeCopyOnSelectPref(next);
  }, []);

  // Panel-owned subscription: buffer ALL sessions (incl. background) even when a
  // view is unmounted; push live chunks into the mounted writer when present.
  useEffect(() => {
    if (!desktopHost) return;

    const unlistenData = subscribeCurrentWebviewEvent<TerminalDataEvent>('terminal-data', (payload) => {
      appendOutput(payload.id, payload.data);
      writersRef.current.get(payload.id)?.(payload.data);
    });

    const unlistenExit = subscribeCurrentWebviewEvent<TerminalExitEvent>('terminal-exit', (payload) => {
      const line = exitStatusLine(tRef.current, payload.code);
      appendOutput(payload.id, line);
      writersRef.current.get(payload.id)?.(line);
      setSessions((prev) =>
        prev.map((s) =>
          s.id === payload.id ? { ...s, exited: true, exitCode: payload.code } : s,
        ),
      );
    });

    return () => {
      unlistenData();
      unlistenExit();
    };
  }, [desktopHost, appendOutput]);

  // Kill PTYs when this panel truly unmounts (office mode / host teardown).
  useEffect(() => {
    return () => {
      for (const s of sessionsRef.current) {
        void killTerminal(s.id).catch(() => {
          /* already exited */
        });
      }
    };
  }, []);

  const createSession = useCallback(async (): Promise<string | null> => {
    if (!desktopHost) return null;
    if (spawnLockRef.current) return null;
    if (sessionsRef.current.length >= MAX_SESSIONS) {
      setError(t('terminal.maxSessions', { max: String(MAX_SESSIONS) }));
      return null;
    }
    spawnLockRef.current = true;
    setSpawning(true);
    setError(null);
    const cwd = workspaceRef.current;
    const shell = shellKindRef.current;
    try {
      const id = await spawnTerminal(cwd, DEFAULT_COLS, DEFAULT_ROWS, {
        shell,
        loadProfile: loadProfileRef.current,
      });
      setSessions((prev) => {
        const title = nextTerminalTitle(prev);
        return [...prev, { id, title, cwd, shell }];
      });
      setActiveId(id);
      buffersRef.current.set(id, '');
      return id;
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      return null;
    } finally {
      spawnLockRef.current = false;
      setSpawning(false);
    }
  }, [desktopHost, t]);

  const closeSession = useCallback(
    async (id: string) => {
      try {
        await killTerminal(id);
      } catch {
        /* session may already have exited */
      }
      buffersRef.current.delete(id);
      writersRef.current.delete(id);
      actionsRef.current.delete(id);
      setSessions((prev) => {
        const next = prev.filter((s) => s.id !== id);
        if (activeId === id) {
          setActiveId(next[0]?.id ?? null);
        }
        return next;
      });
    },
    [activeId],
  );

  const reopenAllInWorkspace = useCallback(async () => {
    const existing = [...sessionsRef.current];
    for (const s of existing) {
      try {
        await killTerminal(s.id);
      } catch {
        /* already exited */
      }
      buffersRef.current.delete(s.id);
      writersRef.current.delete(s.id);
      actionsRef.current.delete(s.id);
    }
    setSessions([]);
    setActiveId(null);
    setCwdBannerDismissed(false);
    await createSession();
  }, [createSession]);

  const clearActive = useCallback(() => {
    if (!activeSession) return;
    actionsRef.current.get(activeSession.id)?.clear();
    buffersRef.current.set(activeSession.id, '');
  }, [activeSession]);

  const findNext = useCallback(() => {
    if (!activeSession || !searchQuery.trim()) return;
    actionsRef.current.get(activeSession.id)?.findNext(searchQuery);
  }, [activeSession, searchQuery]);

  const findPrevious = useCallback(() => {
    if (!activeSession || !searchQuery.trim()) return;
    actionsRef.current.get(activeSession.id)?.findPrevious(searchQuery);
  }, [activeSession, searchQuery]);

  const renameActive = useCallback(() => {
    if (!activeSession) return;
    const next = window.prompt(t('terminal.renamePrompt'), activeSession.title);
    if (next == null) return;
    const trimmed = next.trim();
    if (!trimmed) return;
    setSessions((prev) =>
      prev.map((s) => (s.id === activeSession.id ? { ...s, title: trimmed } : s)),
    );
    setMenuOpen(false);
  }, [activeSession, t]);

  useEffect(() => {
    if (!active || !desktopHost) return;
    if (sessions.length === 0 && !spawning) {
      void createSession();
    }
  }, [active, desktopHost, sessions.length, spawning, createSession]);

  // Parent hotkey (Ctrl+Shift+`): open panel then create another session.
  useEffect(() => {
    if (!desktopHost || createSessionNonce <= 0) return;
    if (createSessionNonce === handledCreateNonceRef.current) return;
    handledCreateNonceRef.current = createSessionNonce;
    void createSession();
  }, [createSessionNonce, desktopHost, createSession]);

  // Parent “Open in Terminal”: reuse a live session (or create one) and cd.
  useEffect(() => {
    if (!desktopHost || cdRequestNonce <= 0) return;
    if (cdRequestNonce === handledCdNonceRef.current) return;
    handledCdNonceRef.current = cdRequestNonce;
    const path = (cdRequestPath ?? '').trim();
    if (!path) return;

    void (async () => {
      const live = sessionsRef.current.find((s) => !s.exited);
      let id = live?.id ?? null;
      let shell = live?.shell ?? shellKindRef.current;
      if (!id) {
        id = await createSession();
        shell = shellKindRef.current;
      }
      if (!id) return;
      setActiveId(id);
      const cmd = buildCdCommand(shell, path, isWindows);
      if (!cmd) return;
      try {
        await writeTerminal(id, cmd);
      } catch {
        /* PTY may have raced to exit */
      }
    })();
  }, [cdRequestNonce, cdRequestPath, desktopHost, createSession, isWindows]);

  useEffect(() => {
    if (!menuOpen) return;
    const onDoc = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setMenuOpen(false);
      }
    };
    document.addEventListener('mousedown', onDoc);
    return () => document.removeEventListener('mousedown', onDoc);
  }, [menuOpen]);

  useEffect(() => {
    if (!active) return;
    return () => {
      setMenuOpen(false);
      setSearchOpen(false);
    };
  }, [active]);

  useEffect(() => {
    if (!searchOpen) return;
    const t = window.setTimeout(() => searchInputRef.current?.focus(), 0);
    return () => window.clearTimeout(t);
  }, [searchOpen]);

  // Ctrl/Cmd+F toggles search when this panel is active.
  useEffect(() => {
    if (!active) return;
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
      if (e.key.toLowerCase() !== 'f') return;
      e.preventDefault();
      e.stopPropagation();
      setSearchOpen((open) => !open);
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [active]);

  if (!desktopHost) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center text-xs text-t-text-muted">
        <p>{t('terminal.desktopOnly')}</p>
      </div>
    );
  }

  const canCloseSession = (s: TerminalSessionMeta) => sessions.length > 1 || Boolean(s.exited);
  const selectClass =
    'max-w-[9rem] truncate rounded border px-1.5 py-1 text-[11px]';
  const chipBtnClass = 'rounded-md px-1.5 py-1 text-[11px] disabled:opacity-40';

  return (
    <div
      className="terminal-panel flex min-h-0 flex-1 flex-col"
      style={{ backgroundColor: chrome.panelBg, color: chrome.text }}
    >
      <div
        className="terminal-panel-header flex shrink-0 flex-col gap-1 px-2 py-1.5"
        style={{ borderBottom: `1px solid ${chrome.headerBorder}` }}
      >
        {sessions.length > 0 && (
          <div
            className="flex min-w-0 items-center gap-0.5 overflow-x-auto"
            role="tablist"
            aria-label={t('terminal.sessionTablist')}
            onKeyDown={onSessionTabListKeyDown}
          >
            {sessions.map((s) => {
              const selected = s.id === activeId;
              const exitTitle = s.exited
                ? t('terminal.exitedBadge', {
                    code: s.exitCode != null ? String(s.exitCode) : '—',
                  })
                : undefined;
              return (
                <div
                  key={s.id}
                  id={terminalTabId(s.id)}
                  role="tab"
                  aria-selected={selected}
                  aria-controls={terminalTabPanelId}
                  tabIndex={selected ? 0 : -1}
                  className="flex max-w-[9rem] shrink-0 items-center gap-0.5 rounded-md px-2 py-1 text-xs"
                  style={{
                    backgroundColor: selected ? chrome.chipActive : 'transparent',
                    color: selected ? chrome.text : chrome.muted,
                  }}
                  onClick={() => setActiveId(s.id)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      setActiveId(s.id);
                    }
                  }}
                >
                  <span className="min-w-0 flex-1 truncate">{s.title}</span>
                  {s.exited && (
                    <span
                      className="inline-flex size-1.5 shrink-0 rounded-full bg-amber-500"
                      title={exitTitle}
                      aria-label={exitTitle}
                    />
                  )}
                  {canCloseSession(s) && (
                    <button
                      type="button"
                      className="rounded px-0.5 hover:opacity-100"
                      style={{ color: chrome.muted }}
                      aria-label={t('terminal.close')}
                      onClick={(e) => {
                        e.stopPropagation();
                        void closeSession(s.id);
                      }}
                    >
                      ×
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        )}
        <div ref={menuRef} className="relative flex min-w-0 flex-1 flex-wrap items-center gap-1">
          <button
            type="button"
            className="flex min-w-0 max-w-[10rem] items-center gap-1 rounded-md px-2 py-1 text-xs font-medium"
            style={{ color: chrome.text }}
            aria-haspopup="menu"
            aria-expanded={menuOpen}
            aria-controls={menuId}
            onClick={() => setMenuOpen((o) => !o)}
          >
            <span className="truncate">{activeSession?.title ?? t('terminal.title')}</span>
            <svg className="size-3.5 shrink-0 opacity-70" viewBox="0 0 16 16" fill="currentColor" aria-hidden>
              <path d="M4 6l4 4 4-4" />
            </svg>
          </button>
          <label className="flex min-w-0 items-center gap-1 text-[10px]" style={{ color: chrome.muted }}>
            <span className="sr-only">{t('terminal.shellLabel')}</span>
            <select
              className={selectClass}
              style={{
                borderColor: chrome.inputBorder,
                backgroundColor: chrome.inputBg,
                color: chrome.text,
              }}
              value={shellKind}
              aria-label={t('terminal.shellLabel')}
              title={t('terminal.shellHint')}
              onChange={(e) => onShellChange(e.target.value as TerminalShellKind)}
            >
              {shellOptions.map((opt) => (
                <option key={opt} value={opt}>
                  {t(shellLabelKey(opt))}
                </option>
              ))}
            </select>
          </label>
          <label className="flex min-w-0 items-center gap-1 text-[10px]" style={{ color: chrome.muted }}>
            <span className="sr-only">{t('terminal.fontSize')}</span>
            <select
              className={selectClass}
              style={{
                borderColor: chrome.inputBorder,
                backgroundColor: chrome.inputBg,
                color: chrome.text,
              }}
              value={fontSize}
              aria-label={t('terminal.fontSize')}
              title={t('terminal.fontSize')}
              onChange={(e) => onFontSizeChange(Number(e.target.value) as TerminalFontSize)}
            >
              {TERMINAL_FONT_SIZES.map((size) => (
                <option key={size} value={size}>
                  {size}px
                </option>
              ))}
            </select>
          </label>
          {showProfileToggle && (
            <label
              className="flex cursor-pointer items-center gap-1 rounded px-1 py-0.5 text-[10px]"
              style={{ color: chrome.muted }}
              title={t('terminal.loadProfileHint')}
            >
              <input
                type="checkbox"
                className="size-3 accent-current"
                checked={loadProfile}
                onChange={(e) => onLoadProfileChange(e.target.checked)}
              />
              <span>{t('terminal.loadProfile')}</span>
            </label>
          )}
          <label
            className="flex cursor-pointer items-center gap-1 rounded px-1 py-0.5 text-[10px]"
            style={{ color: chrome.muted }}
            title={t('terminal.copyOnSelectHint')}
          >
            <input
              type="checkbox"
              className="size-3 accent-current"
              checked={copyOnSelect}
              onChange={(e) => onCopyOnSelectChange(e.target.checked)}
            />
            <span>{t('terminal.copyOnSelect')}</span>
          </label>
          <button
            type="button"
            className={chipBtnClass}
            style={{ color: chrome.muted }}
            title={t('terminal.clear')}
            disabled={!activeSession || activeSession.exited}
            onClick={clearActive}
          >
            {t('terminal.clear')}
          </button>
          <button
            type="button"
            className={chipBtnClass}
            style={{ color: chrome.muted }}
            title={t('terminal.search')}
            disabled={!activeSession}
            aria-pressed={searchOpen}
            onClick={() => setSearchOpen((o) => !o)}
          >
            {t('terminal.search')}
          </button>
          <button
            type="button"
            className="rounded-md p-1 disabled:opacity-40"
            style={{ color: chrome.muted }}
            title={t('terminal.new')}
            disabled={spawning || sessions.length >= MAX_SESSIONS}
            onClick={() => void createSession()}
          >
            <svg className="size-4" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden>
              <path d="M8 3v10M3 8h10" strokeLinecap="round" />
            </svg>
          </button>

          {menuOpen && (
            <div
              id={menuId}
              role="menu"
              className="absolute left-0 top-full z-50 mt-1 min-w-[11rem] rounded-lg py-1 shadow-xl"
              style={{
                border: `1px solid ${chrome.inputBorder}`,
                backgroundColor: chrome.inputBg,
              }}
            >
              <div
                className="px-3 py-1.5 text-[10px] font-medium uppercase tracking-wide"
                style={{ color: chrome.muted }}
              >
                {t('terminal.menuTitle')}
              </div>
              {sessions.map((s) => (
                <button
                  key={s.id}
                  type="button"
                  role="menuitemradio"
                  aria-checked={s.id === activeId}
                  className="flex w-full items-center justify-between gap-2 px-3 py-1.5 text-left text-xs"
                  style={{ color: chrome.text }}
                  onClick={() => {
                    setActiveId(s.id);
                    setMenuOpen(false);
                  }}
                >
                  <span className="flex min-w-0 items-center gap-1.5 truncate">
                    <span className="truncate">{s.title}</span>
                    {s.exited && (
                      <span
                        className="inline-flex size-1.5 shrink-0 rounded-full bg-amber-500"
                        title={t('terminal.exitedBadge', {
                          code: s.exitCode != null ? String(s.exitCode) : '—',
                        })}
                      />
                    )}
                  </span>
                  {s.id === activeId && (
                    <svg
                      className="size-3.5 shrink-0"
                      style={{ color: chrome.muted }}
                      viewBox="0 0 16 16"
                      fill="currentColor"
                      aria-hidden
                    >
                      <path d="M6.2 11.2L3 8l1-1 2.2 2.2L12 3.4l1 1-6.8 6.8z" />
                    </svg>
                  )}
                </button>
              ))}
              <div className="my-1" style={{ borderTop: `1px solid ${chrome.headerBorder}` }} />
              <button
                type="button"
                role="menuitem"
                className="w-full px-3 py-1.5 text-left text-xs"
                style={{ color: chrome.text }}
                onClick={() => {
                  setMenuOpen(false);
                  void createSession();
                }}
              >
                {t('terminal.new')}
              </button>
              <button
                type="button"
                role="menuitem"
                className="w-full px-3 py-1.5 text-left text-xs disabled:opacity-40"
                style={{ color: chrome.text }}
                disabled={!activeSession}
                onClick={renameActive}
              >
                {t('terminal.rename')}
              </button>
              {activeSession && canCloseSession(activeSession) && (
                <button
                  type="button"
                  role="menuitem"
                  className="w-full px-3 py-1.5 text-left text-xs text-red-400"
                  onClick={() => {
                    setMenuOpen(false);
                    void closeSession(activeSession.id);
                  }}
                >
                  {t('terminal.close')}
                </button>
              )}
            </div>
          )}
        </div>

        {searchOpen && (
          <div className="flex min-w-0 items-center gap-1 pt-0.5">
            <input
              ref={searchInputRef}
              id={searchInputId}
              type="search"
              className="min-w-0 flex-1 rounded border px-2 py-1 text-[11px] placeholder:opacity-60"
              style={{
                borderColor: chrome.inputBorder,
                backgroundColor: chrome.inputBg,
                color: chrome.text,
              }}
              placeholder={t('terminal.searchPlaceholder')}
              value={searchQuery}
              aria-label={t('terminal.search')}
              onChange={(e) => setSearchQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  if (e.shiftKey) findPrevious();
                  else findNext();
                } else if (e.key === 'Escape') {
                  e.preventDefault();
                  setSearchOpen(false);
                }
              }}
            />
            <button
              type="button"
              className="rounded px-1.5 py-1 text-[11px]"
              style={{ color: chrome.text }}
              onClick={findPrevious}
            >
              {t('terminal.searchPrev')}
            </button>
            <button
              type="button"
              className="rounded px-1.5 py-1 text-[11px]"
              style={{ color: chrome.text }}
              onClick={findNext}
            >
              {t('terminal.searchNext')}
            </button>
          </div>
        )}
      </div>

      {showCwdBanner && (
        <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-amber-900/40 bg-amber-950/40 px-3 py-2 text-[11px] text-amber-100">
          <p className="min-w-0 flex-1">
            {t('terminal.cwdStale', { path: shortPath(workspaceRoot) })}
          </p>
          <button
            type="button"
            className="rounded border border-amber-700/60 bg-amber-900/40 px-2 py-1 font-medium text-amber-50 hover:bg-amber-800/50"
            disabled={spawning}
            onClick={() => void reopenAllInWorkspace()}
          >
            {t('terminal.reopenHere')}
          </button>
          <button
            type="button"
            className="rounded px-2 py-1 text-amber-200/80 hover:bg-amber-900/30"
            onClick={() => setCwdBannerDismissed(true)}
          >
            {t('terminal.cwdDismiss')}
          </button>
        </div>
      )}

      {error && (
        <p className="shrink-0 border-b border-red-900/50 bg-red-950/40 px-3 py-2 text-[11px] text-red-300">
          {error}
        </p>
      )}

      <div
        id={terminalTabPanelId}
        className="relative min-h-0 flex-1"
        role="tabpanel"
        aria-labelledby={activeSession ? terminalTabId(activeSession.id) : undefined}
      >
        {sessions.length === 0 && spawning && (
          <p
            className="absolute inset-0 flex items-center justify-center text-xs"
            style={{ color: chrome.muted }}
          >
            {t('terminal.spawning')}
          </p>
        )}
        {activeSession && (
          <div className="absolute inset-0 flex min-h-0 flex-col">
            <InteractiveTerminalView
              key={activeSession.id}
              sessionId={activeSession.id}
              outputBuffer={buffersRef.current.get(activeSession.id) ?? ''}
              theme={xtermTheme}
              fontSize={fontSize}
              copyOnSelect={copyOnSelect}
              registerWriter={registerWriter}
              registerActions={registerActions}
              active={active}
            />
          </div>
        )}
      </div>
    </div>
  );
}
