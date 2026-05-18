import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  postStreamTurn,
  getSessions,
  getSessionDetail,
  resumeSessionThread,
  getThreadDetail,
  patchThread,
  startThreadTurn,
  getThreadEvents,
  postResolveApproval,
  deleteSession,
  persistThreadSession,
  waitForRuntimeReady,
  waitForRuntimeBootReady,
  invalidateRuntimeBootReadyCache,
  probeRuntimeConnection,
  initRuntimeConfig,
  getRuntimeBase,
  fetchJson,
  type RuntimeConnectionState,
  type SessionInfo,
  type SseTurnEvent,
} from './api/client';
import { useT } from './i18n';
import { normalizeDesktopStreamEvent, type NormalizedStreamEvent, type TurnUsage } from './api/streamNormalize';
import ChatView from './components/ChatView';
import Composer, { type ComposerOutboundMessage } from './components/Composer';
import Sidebar from './components/Sidebar';
import ApprovalDialog from './components/ApprovalDialog';
import RightPanel, { type RightPanelView } from './components/RightPanel';
import { loadWorkspaceFileIntoPreview } from './lib/openWorkspaceFile';
import type { PreviewState } from './components/preview/types';
import type { AgentState } from './types/agent';
import useKeyboardShortcuts from './hooks/useKeyboardShortcuts';
import { streamFlagsForRunMode } from './lib/runtimeMode';
import { rebuildMessagesFromThreadEvents } from './lib/chat/rebuildMessagesFromThread';
import { mapSessionDetailToMessages } from './lib/chat/sessionMessages';
import {
  appendCappedToolOutput,
  capToolOutputForDisplay,
  mergeStreamingToolOutput,
  stringifyToolInput,
  toolOutputString,
} from './lib/chat/toolOutput';
import {
  type DesktopModelId,
  type DesktopRouteIntentOption,
  type DesktopRunModeId,
  type DesktopTaskTypePreference,
  type DesktopTaskTypeResolved,
  parseDesktopModelId,
  parseDesktopRouteIntentOption,
  parseDesktopRunModeId,
  parseDesktopTaskTypePreference,
  parseDesktopTaskTypeResolved,
  resolveRouteIntentForApi,
} from './types/desktop';
import {
  applyOfficeDefaultWorkspace,
  fetchDefaultComposerWorkspace,
  isUnsafeComposerWorkspace,
  normalizeWorkspaceForApi,
} from './lib/defaultWorkspace';
import { confirmDialog } from './lib/confirmDialog';
import { coerceRunModeForSession, isOfficeSession } from './lib/taskTypeSession';
import {
  contextUsagePercent,
  contextWindowTokensForModel,
  DEFAULT_CONTEXT_WINDOW_TOKENS,
  resolveContextUsedTokens,
} from './lib/contextUsage';

/**
 * When `/health` + `/v1/sessions` probe is `connected`, these banners are usually stale:
 * the failure was a transient fetch or a race before the sidecar finished starting.
 * (We do not clear generic HTTP 4xx/5xx bodies — those may still be actionable.)
 */
function shouldClearBannerWhenRuntimeConnected(banner: string): boolean {
  if (/token|未授权|401|Bearer|运行时 token/i.test(banner)) {
    return true;
  }
  if (/无法连接本地运行时|仍未连接本地运行时|still not connected|cannot reach local runtime/i.test(banner)) {
    return true;
  }
  const transport = /failed to fetch|load failed|networkerror|network request failed|econnrefused|若刚重启应用|127\.0\.0\.1:\d+/i.test(
    banner,
  );
  if (transport) {
    return true;
  }
  return false;
}

interface Message {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  thinking?: string;
  tools?: ToolCall[];
  isStreaming?: boolean;
}

interface ToolCall {
  id: string;
  name: string;
  input: string;
  output?: string;
  status: 'running' | 'done' | 'error';
}

interface ApprovalState {
  toolCallId: string;
  toolName: string;
  description: string;
}

let msgId = 0;
function nextId() {
  return `msg-${++msgId}`;
}

type Theme = 'light' | 'dark';

function loadRunModePreference(): DesktopRunModeId {
  try {
    return parseDesktopRunModeId(localStorage.getItem('deepseek-desktop-run-mode')) ?? 'agent';
  } catch {
    return 'agent';
  }
}

function loadComposerPrefs(): {
  model: DesktopModelId;
  workspace: string;
} {
  try {
    const wm = parseDesktopModelId(localStorage.getItem('deepseek-desktop-model'));
    const ws = normalizeWorkspaceForApi(
      localStorage.getItem('deepseek-desktop-workspace')?.trim() ?? '',
    );
    const workspace =
      ws.length > 0 && !isUnsafeComposerWorkspace(ws) ? ws : '';
    return {
      model: wm ?? 'deepseek-v4-pro',
      workspace,
    };
  } catch {
    return { model: 'deepseek-v4-pro', workspace: '' };
  }
}

/** First-run or legacy `.` / System32 paths → `<Documents>/DS Pick`. */
async function ensureDefaultComposerWorkspace(
  current: string,
  setWorkspace: (path: string) => void,
): Promise<void> {
  if (current.trim().length > 0 && !isUnsafeComposerWorkspace(current)) {
    return;
  }
  const path = await fetchDefaultComposerWorkspace();
  if (path.trim().length > 0 && !isUnsafeComposerWorkspace(path)) {
    setWorkspace(path);
  }
}

function loadTheme(): Theme {
  try {
    const stored = localStorage.getItem('deepseek-theme');
    if (stored === 'dark' || stored === 'light') return stored;
  } catch {
    /* ignore */
  }
  return 'light';
}

const ACTIVE_SESSION_STORAGE_KEY = 'deepseek-desktop-active-session-id';
const ACTIVE_INSPECTOR_STORAGE_KEY = 'deepseek-desktop-active-inspector';
const ROUTE_INTENT_STORAGE_KEY = 'deepseek-desktop-route-intent';
const SIDEBAR_WIDTH_KEY = 'deepseek-desktop-sidebar-width';
const TASK_TYPE_STORAGE_KEY = 'deepseek-desktop-task-type';

function loadTaskTypePreference(): DesktopTaskTypePreference {
  try {
    return parseDesktopTaskTypePreference(localStorage.getItem(TASK_TYPE_STORAGE_KEY)) ?? 'auto';
  } catch {
    return 'auto';
  }
}

/** Periodically persist session file during streaming (loss reduction vs turn-only persist). */
const SESSION_CHECKPOINT_MS = 18_000;

function loadRouteIntentPreference(): DesktopRouteIntentOption {
  try {
    return parseDesktopRouteIntentOption(localStorage.getItem(ROUTE_INTENT_STORAGE_KEY)) ?? 'off';
  } catch {
    return 'off';
  }
}

function loadStoredActiveSessionId(): string | null {
  try {
    const s = localStorage.getItem(ACTIVE_SESSION_STORAGE_KEY)?.trim();
    return s && s.length > 0 ? s : null;
  } catch {
    return null;
  }
}

function loadStoredInspector(): RightPanelView {
  try {
    let s = localStorage.getItem(ACTIVE_INSPECTOR_STORAGE_KEY);
    if (s === 'automation') {
      s = 'tasks-skills';
      try {
        localStorage.setItem(ACTIVE_INSPECTOR_STORAGE_KEY, 'tasks-skills');
      } catch {
        /* ignore */
      }
    }
    if (
      s === 'workspace' ||
      s === 'api-key' ||
      s === 'settings' ||
      s === 'mcp' ||
      s === 'usage' ||
      s === 'tasks-skills' ||
      s === 'agents' ||
      s === 'routing'
    ) {
      return s;
    }
  } catch {
    /* ignore */
  }
  return 'workspace';
}

function applyTheme(theme: Theme) {
  const root = document.documentElement;
  if (theme === 'dark') {
    root.classList.add('dark');
  } else {
    root.classList.remove('dark');
  }
}

export default function App() {
  const { t } = useT();
  const [theme, setTheme] = useState<Theme>(loadTheme);
  const [platform, setPlatform] = useState('unknown');
  const [selectedModel, setSelectedModel] = useState<DesktopModelId>(() => loadComposerPrefs().model);
  const [selectedWorkspace, setSelectedWorkspace] = useState(() => loadComposerPrefs().workspace);
  const [messages, setMessages] = useState<Message[]>([]);
  const [streaming, setStreaming] = useState(false);
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [activeInspector, setActiveInspector] = useState<RightPanelView>(() => loadStoredInspector());
  const [banner, setBanner] = useState<string | null>(null);
  const [resumedThreadId, setResumedThreadId] = useState<string | null>(null);
  const [threadTrustMode, setThreadTrustMode] = useState(false);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [autoApprove, setAutoApprove] = useState(true);
  const [runMode, setRunMode] = useState<DesktopRunModeId>(() => loadRunModePreference());
  const [taskTypePreference, setTaskTypePreference] = useState<DesktopTaskTypePreference>(
    () => loadTaskTypePreference(),
  );
  const [lockedThreadTaskType, setLockedThreadTaskType] = useState<DesktopTaskTypeResolved | null>(
    null,
  );
  const [routeIntent, setRouteIntent] = useState<DesktopRouteIntentOption>(() => loadRouteIntentPreference());
  const [approval, setApproval] = useState<ApprovalState | null>(null);
  const [approvalBusy, setApprovalBusy] = useState(false);
  const [panelPreview, setPanelPreview] = useState<PreviewState | null>(null);
  const [focusWorkspaceFilesNonce, setFocusWorkspaceFilesNonce] = useState(0);
  const [focusWorkspaceDiffNonce, setFocusWorkspaceDiffNonce] = useState(0);
  const [agentStates, setAgentStates] = useState<AgentState[]>([]);
  const [contextWindowTokens, setContextWindowTokens] = useState(DEFAULT_CONTEXT_WINDOW_TOKENS);
  /** Thread detail for empty-transcript fallback only (not summed for context %). */
  const [threadDetailForContext, setThreadDetailForContext] = useState<
    import('./lib/contextUsage').ThreadDetailWithTurns | null
  >(null);
  /** Last completed turn output tokens (Claude-style “↓ N tokens” hint). */
  const [lastTurnOutputTokens, setLastTurnOutputTokens] = useState<number | null>(null);

  const contextUsedTokens = useMemo(
    () => resolveContextUsedTokens(messages, threadDetailForContext, contextWindowTokens),
    [messages, threadDetailForContext, contextWindowTokens],
  );

  const contextUsagePct = useMemo(
    () => contextUsagePercent(contextUsedTokens, 0, contextWindowTokens),
    [contextUsedTokens, contextWindowTokens],
  );

  const [desktopHost, setDesktopHost] = useState(false);
  const [desktopApiKeyConfigured, setDesktopApiKeyConfigured] = useState<boolean | null>(null);
  const [runtimeConn, setRuntimeConn] = useState<RuntimeConnectionState>('checking');
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [rightPanelCollapsed, setRightPanelCollapsed] = useState(false);
  const [sidebarWidth, setSidebarWidthState] = useState(() => {
    try {
      const n = parseInt(localStorage.getItem(SIDEBAR_WIDTH_KEY) ?? '', 10);
      if (Number.isFinite(n) && n >= 180 && n <= 480) return n;
    } catch { /* ignore */ }
    return 240;
  });

  const setSidebarWidthPersisted = useCallback((px: number) => {
    setSidebarWidthState(px);
    try { localStorage.setItem(SIDEBAR_WIDTH_KEY, String(px)); } catch { /* ignore */ }
  }, []);

  const toggleDevtools = useCallback(() => {
    if (!desktopHost) return;
    void import('@tauri-apps/api/core').then(({ invoke }) =>
      invoke('plugin:webview|internal_toggle_devtools'),
    );
  }, [desktopHost]);

  useKeyboardShortcuts([
    { key: 'k', ctrl: true, description: t('keyboard.newSession'), handler: () => handleNewSession() },
    { key: 'n', ctrl: true, description: t('keyboard.workspace'), handler: () => setActiveInspector('workspace') },
    { key: 'f12', global: true, description: t('keyboard.devtools'), handler: () => toggleDevtools() },
    {
      key: 'i',
      ctrl: true,
      shift: true,
      global: true,
      description: t('keyboard.devtools'),
      handler: () => toggleDevtools(),
    },
  ]);

  const eventAbortRef = useRef<AbortController | null>(null);
  const threadTurnRef = useRef<{ threadId: string; turnId: string }>({
    threadId: '',
    turnId: '',
  });
  const activeSessionIdRef = useRef<string | null>(null);
  const lastPersistedTurnRef = useRef<string>('');
  const selectSessionGenerationRef = useRef(0);
  const startupSessionRestoredRef = useRef(false);
  const toolProgressPendingRef = useRef('');
  const toolProgressRafRef = useRef<number | null>(null);
  const selectSessionAbortRef = useRef<AbortController | null>(null);

  useEffect(() => {
    activeSessionIdRef.current = activeSessionId;
  }, [activeSessionId]);

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  useEffect(() => {
    try {
      localStorage.setItem('deepseek-desktop-model', selectedModel);
    } catch {
      /* ignore */
    }
  }, [selectedModel]);

  useEffect(() => {
    const ws = selectedWorkspace.trim();
    if (!ws) return;
    try {
      localStorage.setItem('deepseek-desktop-workspace', ws);
    } catch {
      /* ignore */
    }
  }, [selectedWorkspace]);

  useEffect(() => {
    void ensureDefaultComposerWorkspace(selectedWorkspace, setSelectedWorkspace);
  }, []);

  useEffect(() => {
    try {
      localStorage.setItem('deepseek-desktop-run-mode', runMode);
    } catch {
      /* ignore */
    }
  }, [runMode]);

  useEffect(() => {
    try {
      localStorage.setItem(TASK_TYPE_STORAGE_KEY, taskTypePreference);
    } catch {
      /* ignore */
    }
  }, [taskTypePreference]);

  const officeSession = isOfficeSession(
    taskTypePreference,
    lockedThreadTaskType,
    Boolean(resumedThreadId),
  );

  useEffect(() => {
    if (!resumedThreadId) {
      setLockedThreadTaskType(null);
      return;
    }
    let cancelled = false;
    void getThreadDetail(resumedThreadId).then((detail) => {
      if (cancelled) return;
      const resolved = parseDesktopTaskTypeResolved(detail.thread.task_type);
      setLockedThreadTaskType(resolved ?? 'code');
    });
    return () => {
      cancelled = true;
    };
  }, [resumedThreadId]);

  useEffect(() => {
    try {
      localStorage.setItem(ROUTE_INTENT_STORAGE_KEY, routeIntent);
    } catch {
      /* ignore */
    }
  }, [routeIntent]);

  useEffect(() => {
    try {
      localStorage.setItem(ACTIVE_INSPECTOR_STORAGE_KEY, activeInspector);
    } catch {
      /* ignore */
    }
  }, [activeInspector]);

  const toggleTheme = useCallback(() => {
    setTheme((prev) => {
      const next: Theme = prev === 'light' ? 'dark' : 'light';
      try {
        localStorage.setItem('deepseek-theme', next);
      } catch {
        /* ignore */
      }
      return next;
    });
  }, []);

  /** Re-sync sidebar runtime dot; if probe is OK, drop stale transport-level error banners. */
  const reconcileRuntimeAfterFetchFailure = useCallback(() => {
    void probeRuntimeConnection().then((s) => {
      setRuntimeConn(s);
      if (s === 'connected') {
        setBanner((b) => (!b ? null : shouldClearBannerWhenRuntimeConnected(b) ? null : b));
      }
    });
  }, []);

  const refreshSessions = useCallback(async () => {
    try {
      const list = await getSessions();
      setSessions(list);
      setBanner(null);
    } catch (e) {
      const err = e as Error & { status?: number };
      if (err.status === 401) {
        setBanner(t('banner.unauthorized'));
      } else {
        setBanner(t('banner.loadSessionsError', { message: err.message }));
      }
      reconcileRuntimeAfterFetchFailure();
    }
  }, [reconcileRuntimeAfterFetchFailure]);

  /** Checkpoint session JSON during long streams / tab hide (best-effort). */
  useEffect(() => {
    if (!streaming || !resumedThreadId) {
      return;
    }
    const tid = resumedThreadId;
    const tick = () => {
      void (async () => {
        try {
          const res = await persistThreadSession(tid, activeSessionIdRef.current);
          setActiveSessionId(res.session_id);
          try {
            localStorage.setItem(ACTIVE_SESSION_STORAGE_KEY, res.session_id);
          } catch {
            /* ignore */
          }
          await refreshSessions();
        } catch {
          /* avoid toast spam — turn-complete persist will retry */
        }
      })();
    };
    const id = window.setInterval(tick, SESSION_CHECKPOINT_MS);
    return () => window.clearInterval(id);
  }, [streaming, resumedThreadId, refreshSessions]);

  useEffect(() => {
    const onVis = () => {
      if (document.visibilityState !== 'hidden') {
        return;
      }
      if (!streaming || !resumedThreadId) {
        return;
      }
      const tid = resumedThreadId;
      void (async () => {
        try {
          const res = await persistThreadSession(tid, activeSessionIdRef.current);
          setActiveSessionId(res.session_id);
          try {
            localStorage.setItem(ACTIVE_SESSION_STORAGE_KEY, res.session_id);
          } catch {
            /* ignore */
          }
          await refreshSessions();
        } catch {
          /* ignore */
        }
      })();
    };
    document.addEventListener('visibilitychange', onVis);
    return () => document.removeEventListener('visibilitychange', onVis);
  }, [streaming, resumedThreadId, refreshSessions]);

  const retryConnectAndSessions = useCallback(async () => {
    setBanner(null);
    setRuntimeConn('checking');
    try {
      invalidateRuntimeBootReadyCache();
      await initRuntimeConfig();
      const runtimeUrl = getRuntimeBase();
      const ok = await waitForRuntimeReady({ timeoutMs: 60_000, intervalMs: 150 });
      const probed = await probeRuntimeConnection();
      setRuntimeConn(probed);
      if (!ok) {
        setBanner(t('banner.runtimeUnreachableStartup', { url: runtimeUrl }));
        return;
      }
      await refreshSessions();
    } catch (e) {
      setBanner(t('banner.retryFailed', { message: (e as Error).message }));
      setRuntimeConn('offline');
    }
  }, [refreshSessions]);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const ok = await waitForRuntimeBootReady({ timeoutMs: 90_000, intervalMs: 150 });
        if (!cancelled) {
          const probed = await probeRuntimeConnection();
          setRuntimeConn(probed);
        }
        if (cancelled) {
          return;
        }
        if (!ok) {
          setBanner(t('banner.runtimeUnreachable', { url: getRuntimeBase() }));
          return;
        }
        await refreshSessions();
      } catch (e) {
        if (!cancelled) {
          setBanner(t('banner.bootCheckFailed', { message: (e as Error).message }));
          setRuntimeConn('offline');
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [refreshSessions]);

  const refreshApiKeyStatus = useCallback(() => {
    void (async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const s = await invoke<{ configured: boolean }>('get_api_key_status');
        setDesktopHost(true);
        setDesktopApiKeyConfigured(s.configured);
        const info = await invoke<{ os: string; arch: string; version: string }>('get_platform_info');
        setPlatform(info.os);
        await ensureDefaultComposerWorkspace(
          localStorage.getItem('deepseek-desktop-workspace')?.trim() ?? '',
          setSelectedWorkspace,
        );
      } catch {
        setDesktopHost(false);
        setDesktopApiKeyConfigured(null);
      }
    })();
  }, []);

  useEffect(() => {
    refreshApiKeyStatus();
  }, [refreshApiKeyStatus]);

  // ── Startup gate: window starts invisible; sidecar::ready event shows it ──
  useEffect(() => {
    if (!desktopHost) return;
    let timedOut = false;
    const fallback = setTimeout(() => {
      timedOut = true;
      void import('@tauri-apps/api/window')
         .then(({ getCurrentWindow }) => getCurrentWindow().show())
         .catch(() => {});
     }, 5000); // safety net: show anyway after 5s
     void import('@tauri-apps/api/event')
       .then(({ listen }) =>
         listen<Record<string, unknown>>('sidecar://ready', () => {
           clearTimeout(fallback);
           if (!timedOut) {
             void import('@tauri-apps/api/window')
               .then(({ getCurrentWindow }) => getCurrentWindow().show())
               .catch(() => {});
          }
        }),
      )
      .catch(() => {});
    return () => clearTimeout(fallback);
  }, [desktopHost]);

  useEffect(() => {
    let cancelled = false;
    const tick = async () => {
      const s = await probeRuntimeConnection();
      if (cancelled) {
        return;
      }
      setRuntimeConn(s);
      if (s === 'connected') {
        setBanner((b) => {
          if (!b) {
            return null;
          }
          return shouldClearBannerWhenRuntimeConnected(b) ? null : b;
        });
      }
    };
    void tick();
    const id = window.setInterval(() => void tick(), 8000);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, []);

  const handleSelectSession = useCallback(
    async (sessionId: string) => {
      const gen = ++selectSessionGenerationRef.current;
      eventAbortRef.current?.abort();
      selectSessionAbortRef.current?.abort();
      const selectAbort = new AbortController();
      selectSessionAbortRef.current = selectAbort;
      setBanner(null);
      setActiveSessionId(sessionId);
      setResumedThreadId(null);
      setThreadTrustMode(false);
      setPanelPreview(null);
      lastPersistedTurnRef.current = '';
      try {
        const detail = await getSessionDetail(sessionId);
        if (gen !== selectSessionGenerationRef.current) {
          return;
        }
        const resumed = await resumeSessionThread(sessionId);
        if (gen !== selectSessionGenerationRef.current) {
          return;
        }
        const sessionFallback = mapSessionDetailToMessages(detail) as Message[];
        setMessages(sessionFallback);
        setResumedThreadId(resumed.thread_id);
        try {
          const fromThread = await rebuildMessagesFromThreadEvents(resumed.thread_id, {
            signal: selectAbort.signal,
          });
          if (gen !== selectSessionGenerationRef.current) {
            return;
          }
          if (fromThread.length > 0) {
            setMessages(fromThread as Message[]);
          }
        } catch {
          /* runtime offline or empty event log — keep session text fallback */
        }
        threadTurnRef.current = { threadId: resumed.thread_id, turnId: '' };
        try {
          const threadDetail = await getThreadDetail(resumed.thread_id);
          if (gen !== selectSessionGenerationRef.current) {
            return;
          }
          setThreadDetailForContext(threadDetail);
          const turns = threadDetail.turns ?? [];
          const lastTurn = turns.length > 0 ? turns[turns.length - 1] : undefined;
          const lastOut = lastTurn?.usage?.output_tokens;
          setLastTurnOutputTokens(
            lastOut != null && Number.isFinite(lastOut) && lastOut > 0 ? lastOut : null,
          );
          setContextWindowTokens(
            contextWindowTokensForModel(threadDetail.thread.model ?? selectedModel),
          );
          setSelectedWorkspace(threadDetail.thread.workspace);
          setThreadTrustMode(Boolean(threadDetail.thread.trust_mode));
        } catch (syncErr) {
          if (gen !== selectSessionGenerationRef.current) {
            return;
          }
          setThreadDetailForContext(null);
          setContextWindowTokens(contextWindowTokensForModel(selectedModel));
          const errMsg = syncErr instanceof Error ? syncErr.message : String(syncErr);
          setBanner(t('banner.threadWorkspaceError', { errMsg }));
          reconcileRuntimeAfterFetchFailure();
        }
        try {
          localStorage.setItem(ACTIVE_SESSION_STORAGE_KEY, sessionId);
        } catch {
          /* ignore */
        }
      } catch (e) {
        if (gen !== selectSessionGenerationRef.current) {
          return;
        }
        const err = e as Error & { status?: number };
        if (err.status === 401) {
          setBanner(t('banner.unauthorized401'));
        } else {
          setBanner(t('banner.loadSessionFailed', { message: err.message }));
        }
        reconcileRuntimeAfterFetchFailure();
      }
    },
    [reconcileRuntimeAfterFetchFailure, t],
  );

  /** After the sidebar session list loads, re-open the last desktop session (if still present). */
  useEffect(() => {
    if (sessions.length === 0 || startupSessionRestoredRef.current) {
      return;
    }
    const stored = loadStoredActiveSessionId();
    if (!stored) {
      startupSessionRestoredRef.current = true;
      return;
    }
    if (!sessions.some((s) => s.id === stored)) {
      try {
        localStorage.removeItem(ACTIVE_SESSION_STORAGE_KEY);
      } catch {
        /* ignore */
      }
      startupSessionRestoredRef.current = true;
      return;
    }
    startupSessionRestoredRef.current = true;
    void handleSelectSession(stored);
  }, [sessions, handleSelectSession]);

  const handleNewSession = useCallback(() => {
    eventAbortRef.current?.abort();
    selectSessionAbortRef.current?.abort();
    selectSessionGenerationRef.current += 1;
    setMessages([]);
    setResumedThreadId(null);
    setLockedThreadTaskType(null);
    setThreadTrustMode(false);
    setPanelPreview(null);
    setActiveSessionId(null);
    setThreadDetailForContext(null);
    setLastTurnOutputTokens(null);
    setContextWindowTokens(contextWindowTokensForModel(selectedModel));
    try {
      localStorage.removeItem(ACTIVE_SESSION_STORAGE_KEY);
    } catch {
      /* ignore */
    }
    threadTurnRef.current = { threadId: '', turnId: '' };
    lastPersistedTurnRef.current = '';
    setApproval(null);
  }, []);

  useEffect(() => {
    if (!officeSession) return;
    if (
      activeInspector === 'agents' ||
      activeInspector === 'index' ||
      activeInspector === 'checklist' ||
      activeInspector === 'routing'
    ) {
      setActiveInspector('workspace');
    }
  }, [officeSession, activeInspector]);

  /** Office composer uses Documents/DS Pick when not bound to a resumed thread workspace. */
  useEffect(() => {
    if (!officeSession || resumedThreadId) return;
    if (taskTypePreference !== 'office' && lockedThreadTaskType !== 'office') return;
    void applyOfficeDefaultWorkspace(setSelectedWorkspace);
  }, [officeSession, taskTypePreference, lockedThreadTaskType, resumedThreadId]);

  useEffect(() => {
    if (!officeSession) return;
    setRunMode((m) => coerceRunModeForSession(m, true));
  }, [officeSession]);

  const handleTaskTypePreferenceChange = useCallback(
    (next: DesktopTaskTypePreference) => {
      void (async () => {
        if (resumedThreadId) {
          const ok = await confirmDialog(
            '切换任务类型将新建会话，当前对话不会带入。是否继续？',
          );
          if (!ok) return;
          handleNewSession();
        }
        setTaskTypePreference(next);
        if (next === 'office') {
          setRunMode('agent');
          setAutoApprove(true);
          await applyOfficeDefaultWorkspace(setSelectedWorkspace);
        }
      })();
    },
    [resumedThreadId, handleNewSession],
  );

  const handleDeleteSession = useCallback(
    async (sessionId: string) => {
      if (!(await confirmDialog(t('sidebar.deleteConfirm')))) return;
      setBanner(null);
      try {
        await deleteSession(sessionId);
        if (activeSessionId === sessionId) {
          handleNewSession();
        }
        await refreshSessions();
      } catch (e) {
        const err = e as Error & { status?: number };
        setBanner(t('banner.deleteSessionFailed', { message: err.message }));
      }
    },
    [activeSessionId, handleNewSession, refreshSessions, t],
  );

  const handleCancelStream = useCallback(() => {
    eventAbortRef.current?.abort();
    setLastTurnOutputTokens(null);
  }, []);

  const handleComposerWorkspaceChange = useCallback(
    async (next: string) => {
      const trimmed = next.trim();
      if (!trimmed) {
        throw new Error(t('banner.workspaceEmpty'));
      }
      if (!resumedThreadId) {
        setSelectedWorkspace(trimmed);
        return;
      }
      try {
        const updated = await patchThread(resumedThreadId, { workspace: trimmed });
        setSelectedWorkspace(typeof updated.workspace === 'string' ? updated.workspace : trimmed);
      } catch (e) {
        const err = e as Error & { status?: number };
        let msg = err.message ?? String(e);
        if (/active turn|finish or interrupt/i.test(msg)) {
          setBanner(t('banner.activeTurnBlocking'));
        } else {
          setBanner(t('banner.updateThreadWorkspace', { msg }));
        }
        throw err;
      }
    },
    [resumedThreadId, t],
  );

  const closePanelPreview = useCallback(() => {
    setPanelPreview(null);
  }, []);

  const openWorkspaceFileForPreview = useCallback(
    async (relPath: string, title?: string) => {
      if (runtimeConn !== 'connected') {
        throw new Error(t('banner.runtimeNotConnected'));
      }
      setActiveInspector('workspace');
      setFocusWorkspaceFilesNonce((n) => n + 1);
      const state = await loadWorkspaceFileIntoPreview({
        relPath,
        title,
        workspaceRoot: selectedWorkspace,
        resumedThreadId,
        desktopHost,
      });
      setPanelPreview(state);
    },
    [runtimeConn, selectedWorkspace, resumedThreadId, desktopHost, t],
  );

  const handleChatOpenWorkspacePath = useCallback(
    async (relPath: string) => {
      try {
        await openWorkspaceFileForPreview(relPath);
      } catch (e) {
        const err = e instanceof Error ? e.message : String(e);
        setBanner(t('banner.openFileFailed', { err }));
      }
    },
    [openWorkspaceFileForPreview, t],
  );

  const openDiffInPanel = useCallback(() => {
    setActiveInspector('workspace');
    setRightPanelCollapsed(false);
    setFocusWorkspaceDiffNonce((n) => n + 1);
  }, []);

  const handleRequestDiffPanel = useCallback(() => {
    openDiffInPanel();
  }, [openDiffInPanel]);

  const handleExportSessionJson = useCallback(async () => {
    if (!activeSessionId) {
      setBanner(t('banner.exportNoSession'));
      return;
    }
    const sid = activeSessionId;
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const { invoke } = await import('@tauri-apps/api/core');
      const savePath = await save({
        title: t('composer.exportSessionTitle'),
        defaultPath: `deepseek-session-${sid.slice(0, 8)}.json`,
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!savePath) return;
      await invoke('export_session_json', { sessionId: sid, savePath });
    } catch {
      try {
        const data = await getSessionDetail(sid);
        const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `deepseek-session-${sid.slice(0, 8)}.json`;
        a.click();
        URL.revokeObjectURL(url);
      } catch {
        setBanner(t('banner.exportNoData'));
      }
    }
  }, [activeSessionId, t]);

  const handleExportThreadJson = useCallback(async () => {
    if (!resumedThreadId) {
      setBanner(t('banner.exportThreadNoId'));
      return;
    }
    const tid = resumedThreadId;
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const { invoke } = await import('@tauri-apps/api/core');
      const savePath = await save({
        title: t('composer.exportThreadTitle'),
        defaultPath: `deepseek-thread-${tid.slice(0, 8)}.json`,
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!savePath) return;
      await invoke('export_thread_json', { threadId: tid, savePath });
    } catch {
      try {
        const data = await fetchJson(`/v1/threads/${encodeURIComponent(tid)}`);
        const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `deepseek-thread-${tid.slice(0, 8)}.json`;
        a.click();
        URL.revokeObjectURL(url);
      } catch {
        setBanner(t('banner.exportThreadNoData'));
      }
    }
  }, [resumedThreadId, t]);

  const handleSend = useCallback(
    (outbound: ComposerOutboundMessage) => {
      if (!outbound.apiPrompt.trim() || streaming) return;

      eventAbortRef.current?.abort();
      eventAbortRef.current = new AbortController();
      const signal = eventAbortRef.current.signal;

      const userMsg: Message = {
        id: nextId(),
        role: 'user',
        content: outbound.displayContent,
      };
      setMessages((prev) => [...prev, userMsg]);

      const assistantId = nextId();
      const assistantMsg: Message = {
        id: assistantId,
        role: 'assistant',
        content: '',
        isStreaming: true,
      };
      setMessages((prev) => [...prev, assistantMsg]);

      setStreaming(true);
      setBanner(null);
      toolProgressPendingRef.current = '';
      if (toolProgressRafRef.current != null) {
        cancelAnimationFrame(toolProgressRafRef.current);
        toolProgressRafRef.current = null;
      }
      void (async () => {
      const ctx = {
        currentToolId: { current: null as string | null },
      };

      const flushToolProgressToState = () => {
        const chunk = toolProgressPendingRef.current;
        if (!chunk) return;
        toolProgressPendingRef.current = '';
        setMessages((prev) =>
          prev.map((m) => {
            if (m.id !== assistantId) return m;
            const tools = [...(m.tools ?? [])];
            let idx = -1;
            if (ctx.currentToolId.current) {
              idx = tools.findIndex((t) => t.id === ctx.currentToolId.current);
            }
            if (idx < 0) {
              for (let i = tools.length - 1; i >= 0; i--) {
                if (tools[i].status === 'running') {
                  idx = i;
                  break;
                }
              }
            }
            if (idx < 0) return m;
            const t = tools[idx];
            tools[idx] = {
              ...t,
              output: appendCappedToolOutput(t.output ?? '', chunk),
            };
            return { ...m, tools };
          }),
        );
      };

      const scheduleToolProgressFlush = () => {
        if (toolProgressRafRef.current != null) return;
        toolProgressRafRef.current = requestAnimationFrame(() => {
          toolProgressRafRef.current = null;
          flushToolProgressToState();
        });
      };

      let finished = false;
      const finishOnce = () => {
        if (finished) return;
        finished = true;
        if (toolProgressRafRef.current != null) {
          cancelAnimationFrame(toolProgressRafRef.current);
          toolProgressRafRef.current = null;
        }
        flushToolProgressToState();
        setStreaming(false);
        setMessages((prev) =>
          prev.map((m) => (m.id === assistantId ? { ...m, isStreaming: false } : m)),
        );
      };

      const notifyTurnCompleteIfAway = (desktop: boolean) => {
        if (!desktop || !document.hidden) return;
        void (async () => {
          try {
            const mod = await import('@tauri-apps/plugin-notification');
            let granted = await mod.isPermissionGranted();
            if (!granted) {
              const perm = await mod.requestPermission();
              granted = perm === 'granted';
            }
            if (granted) {
              mod.sendNotification({ title: 'DS Pick', body: '模型已完成回答' });
            }
          } catch {
            /* browser mode — not supported */
          }
        })();
      };

      const maybePersistCompletedTurn = () => {
        const { threadId, turnId } = threadTurnRef.current;
        if (!threadId || !turnId || turnId === lastPersistedTurnRef.current) {
          return;
        }
        lastPersistedTurnRef.current = turnId;
        void (async () => {
          try {
            const res = await persistThreadSession(threadId, activeSessionIdRef.current);
            setActiveSessionId(res.session_id);
            try {
              localStorage.setItem(ACTIVE_SESSION_STORAGE_KEY, res.session_id);
            } catch {
              /* ignore */
            }
            await refreshSessions();
          } catch (e) {
            setBanner(t('banner.persistSessionFailed', { message: (e as Error).message }));
          }
        })();
      };

      const applyNorm = (norm: NormalizedStreamEvent) => {
        switch (norm.kind) {
          case 'turn_started':
            threadTurnRef.current = {
              threadId: norm.threadId,
              turnId: norm.turnId,
            };
            if (norm.threadId) {
              setResumedThreadId(norm.threadId);
            }
            break;
          case 'thinking_delta': {
            setMessages((prev) =>
              prev.map((m) => {
                if (m.id !== assistantId) return m;
                return { ...m, thinking: (m.thinking ?? '') + norm.content };
              }),
            );
            break;
          }
          case 'message_delta': {
            setMessages((prev) =>
              prev.map((m) => {
                if (m.id !== assistantId) return m;
                return { ...m, content: m.content + norm.content };
              }),
            );
            break;
          }
          case 'tool_started': {
            ctx.currentToolId.current = norm.id;
            const inputStr = stringifyToolInput(norm.input);
            setMessages((prev) =>
              prev.map((m) => {
                if (m.id !== assistantId) return m;
                const tools = [
                  ...(m.tools ?? []),
                  { id: norm.id, name: norm.name, input: inputStr, status: 'running' as const },
                ];
                return { ...m, tools };
              }),
            );
            break;
          }
          case 'tool_progress': {
            toolProgressPendingRef.current += norm.output;
            scheduleToolProgressFlush();
            break;
          }
          case 'tool_completed': {
            if (toolProgressRafRef.current != null) {
              cancelAnimationFrame(toolProgressRafRef.current);
              toolProgressRafRef.current = null;
            }
            flushToolProgressToState();
            const outStr = capToolOutputForDisplay(toolOutputString(norm.output));
            setMessages((prev) =>
              prev.map((m) => {
                if (m.id !== assistantId) return m;
                const tools = [...(m.tools ?? [])];
                let idx = tools.findIndex((t) => t.id === norm.id);
                if (idx < 0) {
                  for (let i = tools.length - 1; i >= 0; i--) {
                    if (tools[i].status === 'running') {
                      idx = i;
                      break;
                    }
                  }
                }
                if (idx < 0) return m;
                const t = tools[idx];
                const prevOut = (t.output ?? '').trim();
                const finalOut = outStr.trim();
                const merged = capToolOutputForDisplay(
                  mergeStreamingToolOutput(prevOut, finalOut || ''),
                );
                tools[idx] = {
                  ...t,
                  output: merged,
                  status: norm.success ? ('done' as const) : ('error' as const),
                };
                return { ...m, tools };
              }),
            );
            if (ctx.currentToolId.current === norm.id) {
              ctx.currentToolId.current = null;
            }
            break;
          }
          case 'approval_required':
            setApproval({
              toolCallId: norm.id,
              toolName: norm.toolName,
              description: norm.description,
            });
            break;
          case 'turn_completed':
            finishOnce();
            maybePersistCompletedTurn();
            notifyTurnCompleteIfAway(desktopHost);
            if (norm.usage?.output_tokens != null && norm.usage.output_tokens > 0) {
              setLastTurnOutputTokens(norm.usage.output_tokens);
            }
            break;
          case 'done':
            finishOnce();
            maybePersistCompletedTurn();
            notifyTurnCompleteIfAway(desktopHost);
            break;
          case 'error':
            finishOnce();
            setMessages((prev) =>
              prev.map((m) =>
                m.id === assistantId
                  ? { ...m, content: m.content || `Error: ${norm.message}`, isStreaming: false }
                  : m,
              ),
            );
            setBanner(norm.message ? norm.message : t('banner.streamError'));
            break;
          case 'agent_spawned':
            setAgentStates((prev) => {
              const exists = prev.some((a) => a.agentId === norm.agentId);
              if (exists) return prev;
              return [
                ...prev,
                {
                  agentId: norm.agentId,
                  status: 'spawned',
                  toolCalls: [],
                  resultSummary: null,
                  tokens: 0,
                  spawnedAt: Date.now(),
                  completedAt: null,
                },
              ];
            });
            break;
          case 'agent_progress':
            setAgentStates((prev) =>
              prev.map((a) =>
                a.agentId === norm.agentId ? { ...a, status: 'running' as const } : a,
              ),
            );
            break;
          case 'agent_completed':
            setAgentStates((prev) =>
              prev.map((a) =>
                a.agentId === norm.agentId
                  ? { ...a, status: 'completed' as const, resultSummary: norm.result, completedAt: Date.now() }
                  : a,
              ),
            );
            break;
          case 'agent_list':
            setAgentStates((prev) => {
              const now = Date.now();
              return norm.agents.map((a) => {
                const existing = prev.find((e) => e.agentId === a.id);
                if (existing) return existing;
                return {
                  agentId: a.id,
                  status: a.status === 'Completed' ? 'completed' as const
                    : a.status === 'Interrupted' ? 'interrupted' as const
                    : 'running' as const,
                  toolCalls: [],
                  resultSummary: null,
                  tokens: 0,
                  spawnedAt: now,
                  completedAt: a.status === 'Completed' ? now : null,
                };
              });
            });
            break;
          default:
            break;
        }
      };

      const onSseEvent = (ev: SseTurnEvent, filter?: { turnId: string }) => {
        if (signal.aborted) return;
        const norm = normalizeDesktopStreamEvent(ev, filter);
        if (norm) {
          applyNorm(norm);
        }
      };

      const handleHttpError = (err: Error & { status?: number }) => {
        const msg = err.message || String(err);
        const status = err.status;
        if (status === 401) {
          setBanner(t('banner.unauthorizedBearer'));
        } else if (/api\s*key|DEEPSEEK_API_KEY|401|unauthorized/i.test(msg)) {
          setBanner(t('banner.missingApiKey'));
        }
        setMessages((prev) =>
          prev.map((m) =>
            m.id === assistantId
              ? { ...m, content: m.content || `Error: ${msg}`, isStreaming: false }
              : m,
          ),
        );
        finishOnce();
      };

      const streamOpts = streamFlagsForRunMode(runMode, autoApprove);
      const routeIntentApi = resolveRouteIntentForApi(routeIntent, runMode);

      try {
        if (resumedThreadId) {
          const detail = await getThreadDetail(resumedThreadId);
          if (signal.aborted) {
            finishOnce();
            return;
          }
          const sinceSeq = detail.latest_seq ?? 0;
          const { turn } = await startThreadTurn(resumedThreadId, {
            prompt: outbound.apiPrompt,
            model: selectedModel,
            mode: streamOpts.mode,
            allow_shell: streamOpts.allow_shell,
            trust_mode: streamOpts.trust_mode,
            auto_approve: streamOpts.auto_approve,
            ...(routeIntentApi != null ? { route_intent: routeIntentApi } : {}),
            task_type: taskTypePreference,
          });
          if (signal.aborted) {
            finishOnce();
            return;
          }
          const turnId = turn.id;
          threadTurnRef.current = {
            threadId: resumedThreadId,
            turnId,
          };

          await getThreadEvents(
            resumedThreadId,
            sinceSeq,
            (ev) => onSseEvent(ev, { turnId }),
            { signal },
          );
          finishOnce();
        } else {
          await postStreamTurn(
            {
              prompt: outbound.apiPrompt,
              workspace: selectedWorkspace,
              mode: streamOpts.mode,
              model: selectedModel,
              allow_shell: streamOpts.allow_shell,
              trust_mode: streamOpts.trust_mode,
              auto_approve: streamOpts.auto_approve,
              ...(routeIntentApi != null ? { route_intent: routeIntentApi } : {}),
              task_type: taskTypePreference,
            },
            (ev) => onSseEvent(ev),
            () => finishOnce(),
            (err) => handleHttpError(err as Error & { status?: number }),
            { signal },
          );
        }
      } catch (e) {
        if ((e as Error).name === 'AbortError') {
          finishOnce();
          return;
        }
        handleHttpError(e as Error & { status?: number });
      }
      })();
    },
    [
      streaming,
      resumedThreadId,
      autoApprove,
      runMode,
      routeIntent,
      selectedModel,
      selectedWorkspace,
      taskTypePreference,
      refreshSessions,
    ],
  );

  const handleApproveDecision = async (decision: 'approve' | 'deny') => {
    if (!approval) return;
    const { threadId, turnId } = threadTurnRef.current;
    if (!threadId || !turnId) {
      setBanner(t('banner.approvalMissingThread'));
      setApproval(null);
      return;
    }
    setApprovalBusy(true);
    try {
      await postResolveApproval(threadId, turnId, approval.toolCallId, decision);
    } catch (e) {
      const err = e as Error & { status?: number };
      if (err.status === 409) {
        setBanner(t('banner.approvalExpired'));
      } else {
        setBanner(t('banner.approvalSubmitFailed', { message: err.message }));
      }
    } finally {
      setApprovalBusy(false);
      setApproval(null);
    }
  };

  return (
    <div className="flex flex-col h-screen w-screen bg-canvas">
      <TitleBar />
      <div className="flex flex-1 min-h-0 bg-canvas">
      <ApprovalDialog
        open={approval != null}
        toolName={approval?.toolName ?? ''}
        description={approval?.description ?? ''}
        busy={approvalBusy}
        onApprove={() => void handleApproveDecision('approve')}
        onDeny={() => void handleApproveDecision('deny')}
      />
      <Sidebar
        sessions={sessions}
        activeSessionId={activeSessionId}
        onNewSession={handleNewSession}
        onSelectSession={handleSelectSession}
        onDeleteSession={handleDeleteSession}
        desktopHost={desktopHost}
        runtimeConn={runtimeConn}
        apiKeyConfigured={desktopApiKeyConfigured}
        activeInspector={activeInspector}
        onInspectorChange={setActiveInspector}
        sidebarWidth={sidebarWidth}
        onSidebarWidthChange={setSidebarWidthPersisted}
        collapsed={sidebarCollapsed}
        onToggleCollapse={() => setSidebarCollapsed((v) => !v)}
        officeSession={officeSession}
      />
      {/* left toggle strip — visible when sidebar collapsed */}
      {sidebarCollapsed && (
        <button
          type="button"
          onClick={() => setSidebarCollapsed(false)}
          className="shrink-0 w-8 border-r border-rail-edge bg-canvas hover:bg-hover transition-colors flex items-center justify-center group"
          title="展开侧边栏"
        >
          <svg className="w-3.5 h-3.5 text-t-text-muted group-hover:text-t-text transition-colors" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5">
            <path d="M5 4l6 4-6 4" strokeLinecap="round" strokeLinejoin="round"/>
          </svg>
        </button>
      )}
      <div className="flex min-h-0 flex-1 flex-col min-w-0 bg-canvas">
        {banner && (
          <div className="shrink-0 border-b border-divider bg-amber-bg px-4 py-2 text-sm text-amber-text">
            {banner}
            <button type="button" className="ml-3 underline" onClick={() => setBanner(null)}>
              {t('common.close')}
            </button>
            <button
              type="button"
              className="ml-3 underline"
              onClick={() => void retryConnectAndSessions()}
            >
              {t('common.retryConnection')}
            </button>
          </div>
        )}
        {resumedThreadId && (
          <p className="shrink-0 px-4 py-1 text-xs text-t-text-muted border-b border-divider">
            {t('thread.resumed', { id: resumedThreadId.slice(0, 8) })}
          </p>
        )}
        <ChatView
          messages={messages}
          onOpenWorkspacePath={handleChatOpenWorkspacePath}
          onOpenDiffInPanel={openDiffInPanel}
          onRetryMessage={(content) =>
            handleSend({ displayContent: content, apiPrompt: content })
          }
        />
        <Composer
          onSend={handleSend}
          onCancel={handleCancelStream}
          disabled={streaming}
          autoApprove={autoApprove}
          onAutoApproveChange={setAutoApprove}
          runMode={runMode}
          onRunModeChange={setRunMode}
          taskTypePreference={taskTypePreference}
          lockedThreadTaskType={lockedThreadTaskType}
          onTaskTypePreferenceChange={handleTaskTypePreferenceChange}
          routeIntent={routeIntent}
          onOpenRouting={
            officeSession
              ? undefined
              : () => {
                  setRightPanelCollapsed(false);
                  setActiveInspector('routing');
                }
          }
          sessionExportEnabled={Boolean(activeSessionId)}
          threadExportEnabled={Boolean(resumedThreadId)}
          onExportSessionJson={() => void handleExportSessionJson()}
          onExportThreadJson={() => void handleExportThreadJson()}
          model={selectedModel}
          onModelChange={setSelectedModel}
          workspace={selectedWorkspace}
          onWorkspaceChange={handleComposerWorkspaceChange}
          resumedThreadActive={resumedThreadId != null && resumedThreadId.length > 0}
          contextUsagePct={contextUsagePct}
          contextUsedTokens={contextUsedTokens}
          contextWindowTokens={contextWindowTokens}
          lastTurnOutputTokens={lastTurnOutputTokens}
          officeSession={officeSession}
        />
      </div>
      {/* right panel toggle strip */}
      {!rightPanelCollapsed && (
        <RightPanel
          view={activeInspector}
          officeSession={officeSession}
          desktopHost={desktopHost}
          runtimeConn={runtimeConn}
          apiKeyConfigured={desktopApiKeyConfigured}
          onSavedApiKey={() => {
            refreshApiKeyStatus();
            setBanner(null);
          }}
          theme={theme}
          onToggleTheme={toggleTheme}
          platform={platform}
          workspaceRoot={selectedWorkspace}
          resumedThreadId={resumedThreadId}
          threadTrustMode={threadTrustMode}
          onEnableTrust={async () => {
            if (!resumedThreadId) return;
            try {
              await patchThread(resumedThreadId, { trust_mode: true });
              setThreadTrustMode(true);
              setBanner(null);
            } catch (e) {
              const err = e as Error & { status?: number };
              setBanner(t('banner.trustModeFailed', { message: err.message }));
            }
          }}
          preview={panelPreview}
          onClosePreview={closePanelPreview}
          openWorkspaceFile={openWorkspaceFileForPreview}
          focusFilesNonce={focusWorkspaceFilesNonce}
          focusDiffNonce={focusWorkspaceDiffNonce}
          agentStates={agentStates}
          onRequestChecklist={() => setActiveInspector('checklist')}
          messages={messages}
          onRequestMermaid={() => setActiveInspector('mermaid')}
          onRequestDiff={handleRequestDiffPanel}
          onCollapse={() => setRightPanelCollapsed(true)}
          routeIntent={routeIntent}
          onRouteIntentChange={setRouteIntent}
        />
      )}
      {rightPanelCollapsed && (
        <button
          type="button"
          onClick={() => setRightPanelCollapsed(false)}
          className="shrink-0 w-8 border-l border-rail-edge bg-canvas hover:bg-hover transition-colors flex items-center justify-center group"
          title="展开面板"
        >
          <svg className="w-3.5 h-3.5 text-t-text-muted group-hover:text-t-text transition-colors" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5">
            <path d="M5 4l6 4-6 4" strokeLinecap="round" strokeLinejoin="round"/>
          </svg>
        </button>
      )}
    </div>
      </div>
  );
}

function TitleBar() {
  const { t } = useT();
  const handleMinimize = () => {
    void import('@tauri-apps/api/window').then(({ getCurrentWindow }) => getCurrentWindow().minimize());
  };
  const handleToggleMaximize = () => {
    void import('@tauri-apps/api/window').then(async ({ getCurrentWindow }) => {
      const w = getCurrentWindow();
      const max = await w.isMaximized();
      if (max) await w.unmaximize();
      else await w.maximize();
    });
  };
  const handleClose = () => {
    void import('@tauri-apps/api/window').then(({ getCurrentWindow }) => getCurrentWindow().hide());
  };

  return (
    <div
      data-tauri-drag-region
      className="flex items-center h-9 shrink-0 bg-canvas border-b border-divider select-none"
    >
      <span className="pl-3 text-[11px] font-semibold text-t-text-secondary">DS Pick</span>
      <div className="flex-1 min-w-8" data-tauri-drag-region />
      <button
        type="button"
        data-tauri-drag-region="false"
        onClick={handleMinimize}
        className="px-3 py-2 text-t-text-muted hover:text-t-text hover:bg-hover transition-colors"
        aria-label={t('titlebar.minimize')}
      >
        <svg viewBox="0 0 24 24" className="w-3.5 h-3.5 stroke-current" style={{ fill: 'none', strokeWidth: 1.6 }}>
          <path d="M5 12h14" />
        </svg>
      </button>
      <button
        type="button"
        data-tauri-drag-region="false"
        onClick={handleToggleMaximize}
        className="px-3 py-2 text-t-text-muted hover:text-t-text hover:bg-hover transition-colors"
        aria-label={t('titlebar.maximize')}
      >
        <svg viewBox="0 0 24 24" className="w-3.5 h-3.5 stroke-current" style={{ fill: 'none', strokeWidth: 1.6 }}>
          <path d="M4 4h16v16H4z" />
        </svg>
      </button>
      <button
        type="button"
        data-tauri-drag-region="false"
        onClick={handleClose}
        className="px-3 py-2 text-t-text-muted hover:text-white hover:bg-t-error transition-colors"
        aria-label={t('titlebar.close')}
      >
        <svg viewBox="0 0 24 24" className="w-3.5 h-3.5 stroke-current" style={{ fill: 'none', strokeWidth: 1.6 }}>
          <path d="M18 6L6 18M6 6l12 12" />
        </svg>
      </button>
    </div>
  );
}