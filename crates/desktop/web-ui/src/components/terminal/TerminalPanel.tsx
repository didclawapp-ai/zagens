import { useCallback, useEffect, useId, useMemo, useRef, useState } from 'react';
import { useT } from '../../i18n';
import { handleTabListKeyDown } from '../../lib/a11y/rovingTabList';
import { killTerminal, spawnTerminal } from '../../lib/terminal/ptyApi';
import InteractiveTerminalView from './InteractiveTerminalView';

/** Must match MAX_SESSIONS_PER_WINDOW in crates/desktop/src/terminal.rs */
const MAX_SESSIONS = 4;
const DEFAULT_COLS = 80;
const DEFAULT_ROWS = 24;

export interface TerminalSessionMeta {
  id: string;
  title: string;
}

interface Props {
  workspaceRoot: string;
  desktopHost: boolean;
  /** Panel tab is visible — spawn / fit when true */
  active: boolean;
}

function nextTerminalTitle(existing: TerminalSessionMeta[]): string {
  const used = new Set(existing.map((s) => s.title));
  let n = existing.length + 1;
  while (used.has(`Terminal ${n}`)) {
    n += 1;
  }
  return `Terminal ${n}`;
}

export default function TerminalPanel({ workspaceRoot, desktopHost, active }: Props) {
  const { t } = useT();
  const menuId = useId();
  const [sessions, setSessions] = useState<TerminalSessionMeta[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [spawning, setSpawning] = useState(false);
  const buffersRef = useRef<Map<string, string>>(new Map());
  const menuRef = useRef<HTMLDivElement>(null);
  const workspaceRef = useRef(workspaceRoot);
  workspaceRef.current = workspaceRoot;

  const activeSession = sessions.find((s) => s.id === activeId) ?? sessions[0] ?? null;
  const sessionIds = useMemo(() => sessions.map((s) => s.id), [sessions]);
  const terminalTabId = (sessionId: string) => `terminal-tab-${sessionId}`;
  const terminalTabPanelId = 'terminal-tabpanel';

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

  const onTerminalExit = useCallback((_sessionId: string, _code: number | null) => {
    /* keep view; user can close tab manually */
  }, []);

  const createSession = useCallback(async () => {
    if (!desktopHost) return;
    if (sessions.length >= MAX_SESSIONS) {
      setError(t('terminal.maxSessions', { max: String(MAX_SESSIONS) }));
      return;
    }
    setSpawning(true);
    setError(null);
    try {
      const id = await spawnTerminal(workspaceRef.current, DEFAULT_COLS, DEFAULT_ROWS);
      setSessions((prev) => {
        const title = nextTerminalTitle(prev);
        return [...prev, { id, title }];
      });
      setActiveId(id);
      buffersRef.current.set(id, '');
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSpawning(false);
    }
  }, [desktopHost, sessions.length, t]);

  const closeSession = useCallback(
    async (id: string) => {
      try {
        await killTerminal(id);
      } catch {
        /* session may already have exited */
      }
      buffersRef.current.delete(id);
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
    };
  }, [active]);

  if (!desktopHost) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center text-xs text-t-text-muted">
        <p>{t('terminal.desktopOnly')}</p>
      </div>
    );
  }

  return (
    <div className="terminal-panel flex min-h-0 flex-1 flex-col bg-[#121212] text-zinc-200">
      <div className="terminal-panel-header flex shrink-0 flex-col gap-1 border-b border-zinc-800 px-2 py-1.5">
        {sessions.length > 0 && (
          <div
            className="flex min-w-0 items-center gap-0.5 overflow-x-auto"
            role="tablist"
            aria-label={t('terminal.sessionTablist')}
            onKeyDown={onSessionTabListKeyDown}
          >
            {sessions.map((s) => {
              const selected = s.id === activeId;
              return (
                <div
                  key={s.id}
                  id={terminalTabId(s.id)}
                  role="tab"
                  aria-selected={selected}
                  aria-controls={terminalTabPanelId}
                  tabIndex={selected ? 0 : -1}
                  className={`flex max-w-[9rem] shrink-0 items-center gap-0.5 rounded-md px-2 py-1 text-xs ${
                    selected
                      ? 'bg-zinc-800 text-zinc-100'
                      : 'text-zinc-400 hover:bg-zinc-800/60 hover:text-zinc-200'
                  }`}
                  onClick={() => setActiveId(s.id)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      setActiveId(s.id);
                    }
                  }}
                >
                  <span className="min-w-0 flex-1 truncate">{s.title}</span>
                  {sessions.length > 1 && (
                    <button
                      type="button"
                      className="rounded px-0.5 text-zinc-500 hover:bg-zinc-700 hover:text-zinc-200"
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
        <div ref={menuRef} className="relative flex min-w-0 flex-1 items-center gap-1">
          <button
            type="button"
            className="flex min-w-0 max-w-full items-center gap-1 rounded-md px-2 py-1 text-xs font-medium text-zinc-200 hover:bg-zinc-800/80"
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
          <button
            type="button"
            className="rounded-md p-1 text-zinc-400 hover:bg-zinc-800 hover:text-zinc-100"
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
              className="absolute left-0 top-full z-50 mt-1 min-w-[11rem] rounded-lg border border-zinc-700 bg-[#1e1e1e] py-1 shadow-xl"
            >
              <div className="px-3 py-1.5 text-[10px] font-medium uppercase tracking-wide text-zinc-500">
                {t('terminal.menuTitle')}
              </div>
              {sessions.map((s) => (
                <button
                  key={s.id}
                  type="button"
                  role="menuitemradio"
                  aria-checked={s.id === activeId}
                  className="flex w-full items-center justify-between gap-2 px-3 py-1.5 text-left text-xs text-zinc-200 hover:bg-zinc-800"
                  onClick={() => {
                    setActiveId(s.id);
                    setMenuOpen(false);
                  }}
                >
                  <span className="truncate">{s.title}</span>
                  {s.id === activeId && (
                    <svg className="size-3.5 shrink-0 text-zinc-400" viewBox="0 0 16 16" fill="currentColor" aria-hidden>
                      <path d="M6.2 11.2L3 8l1-1 2.2 2.2L12 3.4l1 1-6.8 6.8z" />
                    </svg>
                  )}
                </button>
              ))}
              <div className="my-1 border-t border-zinc-700" />
              <button
                type="button"
                role="menuitem"
                className="w-full px-3 py-1.5 text-left text-xs text-zinc-200 hover:bg-zinc-800"
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
                className="w-full px-3 py-1.5 text-left text-xs text-zinc-200 hover:bg-zinc-800 disabled:opacity-40"
                disabled={!activeSession}
                onClick={renameActive}
              >
                {t('terminal.rename')}
              </button>
              {activeSession && sessions.length > 1 && (
                <button
                  type="button"
                  role="menuitem"
                  className="w-full px-3 py-1.5 text-left text-xs text-red-300 hover:bg-zinc-800"
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
      </div>

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
          <p className="absolute inset-0 flex items-center justify-center text-xs text-zinc-500">
            {t('terminal.spawning')}
          </p>
        )}
        {activeSession && (
          <div className="absolute inset-0 flex min-h-0 flex-col">
            <InteractiveTerminalView
              key={activeSession.id}
              sessionId={activeSession.id}
              outputBuffer={buffersRef.current.get(activeSession.id) ?? ''}
              onOutput={appendOutput}
              onExit={onTerminalExit}
              active={active}
            />
          </div>
        )}
      </div>
    </div>
  );
}
