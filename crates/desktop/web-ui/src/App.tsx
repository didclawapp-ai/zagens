import { useCallback, useEffect, useMemo, useRef, useState, type Dispatch, type SetStateAction } from 'react';
import { useInspectorUnread } from './lib/useInspectorUnread';
import {
  getSessionDetail,
  resumeSessionThread,
  getThreadDetail,
  getThreadContext,
  patchThread,
  forkThreadAtUserMessage,
  waitForRuntimeBootReady,
  getRuntimeBase,
  fetchJson,
  type SystemSettings,
} from './api/client';
import { useT } from './i18n';
import ChatView from './components/ChatView';
import { useAuditNavActivity } from './lib/useAuditNavActivity';
import Composer from './components/Composer';
import ModelParamsDialog, { type ModelParams } from './components/ModelParamsDialog';
import {
  loadModelParams,
  saveModelParams,
} from './lib/modelParams';
import Sidebar from './components/Sidebar';
import ApprovalDialog from './components/ApprovalDialog';
import RightPanel, { type RightPanelView } from './components/RightPanel';
import { loadWorkspaceFileIntoPreview, normalizeWorkspaceRelPath } from './lib/openWorkspaceFile';
import {
  createAgentWindow,
  initWindowContext,
  registerWindowThread,
  updateWindowTitle,
  getWindowLabel,
  workspaceStorageKey,
} from './lib/windowBridge';
import { formatWorkspaceFileError } from './lib/workspaceFileOpenError';
import type { PreviewState } from './components/preview/types';
import useKeyboardShortcuts from './hooks/useKeyboardShortcuts';
import { useRuntimeConnection } from './hooks/useRuntimeConnection';
import { useAgentPanelState } from './hooks/useAgentPanelState';
import { ACTIVE_SESSION_STORAGE_KEY, useTurnSession } from './hooks/useTurnSession';
import { useTurnApproval, type ApprovalState } from './hooks/useTurnApproval';
import { useTurnStream } from './hooks/useTurnStream';
import { useTurnSend } from './hooks/useTurnSend';
import SkipToMainLink from './components/SkipToMainLink';
import { rebuildMessagesFromThreadEvents } from './lib/chat/rebuildMessagesFromThread';
import { depthFromTailForUserMessage } from './lib/chat/backtrackDepth';
import {
  cacheSessionUiMessages,
  getCachedSessionUiMessages,
  type CachedUiMessage,
} from './lib/chat/sessionUiCache';
import { mapSessionDetailToMessages } from './lib/chat/sessionMessages';
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
} from './types/desktop';
import {
  applyOfficeDefaultWorkspace,
  fetchDefaultComposerWorkspace,
  isUnsafeComposerWorkspace,
  normalizeWorkspaceForApi,
  workspacesMatch,
} from './lib/defaultWorkspace';
import { confirmDialog } from './lib/confirmDialog';
import { toast } from './lib/toast';
import { coerceRunModeForSession, isOfficeSession } from './lib/taskTypeSession';
import {
  contextWindowTokensForModel,
  DEFAULT_CONTEXT_WINDOW_TOKENS,
  resolveContextUsedTokens,
  resolveContextUsagePercent,
  type ThreadContextSnapshot,
} from './lib/contextUsage';
import { THREAD_CONTEXT_POLL_STREAMING_MS } from './lib/runtimePoll';
import { isRuntimeApiAvailable } from './lib/runtimeReachable';

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

type Theme = 'light' | 'dark';

function loadRunModePreference(): DesktopRunModeId {
  try {
    return parseDesktopRunModeId(localStorage.getItem('deepseek-desktop-run-mode')) ?? 'agent';
  } catch {
    return 'agent';
  }
}

function loadComposerPrefs(windowLabel: string): {
  model: DesktopModelId;
  workspace: string;
} {
  try {
    const wm = parseDesktopModelId(localStorage.getItem('deepseek-desktop-model'));
    const ws = normalizeWorkspaceForApi(
      localStorage.getItem(workspaceStorageKey(windowLabel))?.trim() ?? '',
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

/** First-run or legacy `.` / System32 paths → `<Documents>/Zagens` (or legacy Zagens folder). */
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

const ACTIVE_INSPECTOR_STORAGE_KEY = 'deepseek-desktop-active-inspector';
const RIGHT_PANEL_COLLAPSED_STORAGE_KEY = 'deepseek-desktop-right-panel-collapsed';
const ROUTE_INTENT_STORAGE_KEY = 'deepseek-desktop-route-intent';
const TASK_TYPE_STORAGE_KEY = 'deepseek-desktop-task-type';

function loadTaskTypePreference(): DesktopTaskTypePreference {
  try {
    return parseDesktopTaskTypePreference(localStorage.getItem(TASK_TYPE_STORAGE_KEY)) ?? 'auto';
  } catch {
    return 'auto';
  }
}

function loadRouteIntentPreference(): DesktopRouteIntentOption {
  try {
    return parseDesktopRouteIntentOption(localStorage.getItem(ROUTE_INTENT_STORAGE_KEY)) ?? 'off';
  } catch {
    return 'off';
  }
}

function loadStoredInspector(): RightPanelView {
  try {
    let s = localStorage.getItem(ACTIVE_INSPECTOR_STORAGE_KEY);
    if (s === 'automation' || s === 'tasks-skills') {
      s = 'tasks';
      try {
        localStorage.setItem(ACTIVE_INSPECTOR_STORAGE_KEY, 'tasks');
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
      s === 'tasks' ||
      s === 'skills' ||
      s === 'agents' ||
      s === 'routing' ||
      s === 'index' ||
      s === 'checklist' ||
      s === 'audit' ||
      s === 'mermaid' ||
      s === 'about'
    ) {
      return s;
    }
  } catch {
    /* ignore */
  }
  return 'workspace';
}

/** First launch (no key): collapsed; later launches restore last collapsed/expanded state. */
function loadStoredRightPanelCollapsed(): boolean {
  try {
    const s = localStorage.getItem(RIGHT_PANEL_COLLAPSED_STORAGE_KEY);
    if (s === null) return true;
    if (s === 'false' || s === '0') return false;
    if (s === 'true' || s === '1') return true;
  } catch {
    /* ignore */
  }
  return true;
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
  const [windowLabel, setWindowLabel] = useState('dev');
  const [showAllSessions, setShowAllSessions] = useState(false);
  const [selectedModel, setSelectedModel] = useState<DesktopModelId>(() => loadComposerPrefs('main').model);
  const [selectedWorkspace, setSelectedWorkspace] = useState(() => loadComposerPrefs('main').workspace);
  const [messages, setMessages] = useState<Message[]>([]);
  const [desktopHost, setDesktopHost] = useState(false);
  const [activeInspector, setActiveInspector] = useState<RightPanelView>(() => loadStoredInspector());
  const [threadTrustMode, setThreadTrustMode] = useState(false);
  const [runMode, setRunMode] = useState<DesktopRunModeId>(() => loadRunModePreference());
  const runModeRef = useRef(runMode);
  const [taskTypePreference, setTaskTypePreference] = useState<DesktopTaskTypePreference>(
    () => loadTaskTypePreference(),
  );
  const [lockedThreadTaskType, setLockedThreadTaskType] = useState<DesktopTaskTypeResolved | null>(
    null,
  );
  const [routeIntent, setRouteIntent] = useState<DesktopRouteIntentOption>(() => loadRouteIntentPreference());

  const refreshSessionsRef = useRef<() => Promise<void>>(async () => {});
  const setRuntimeSessionEstablishedRef = useRef<Dispatch<SetStateAction<boolean>>>(() => {});
  const notifyRuntimeTransientRef = useRef<(message: string) => void>(() => {});
  const reconcileRuntimeAfterFetchFailureRef = useRef<() => void>(() => {});
  const handleSelectSessionRef = useRef<(sessionId: string) => void>(() => {});
  const handleNewSessionRef = useRef<() => void>(() => {});
  const streamingRef = useRef(false);
  const setApprovalRef = useRef<(value: ApprovalState | null) => void>(() => {});
  const setLastTurnOutputTokensRef = useRef<(value: number | null) => void>(() => {});

  const [lastTurnOutputTokens, setLastTurnOutputTokens] = useState<number | null>(null);

  const {
    sessions,
    activeSessionId,
    setActiveSessionId,
    activeSessionIdRef,
    resumedThreadId,
    setResumedThreadId,
    resumedThreadIdRef,
    visibleSessions,
    refreshSessions,
    handleDeleteSession,
  } = useTurnSession({
    t,
    showAllSessions,
    selectedWorkspace,
    streamingRef,
    setRuntimeSessionEstablished: (value) => {
      setRuntimeSessionEstablishedRef.current(value);
    },
    reconcileRuntimeAfterFetchFailure: () => reconcileRuntimeAfterFetchFailureRef.current(),
    notifyRuntimeTransient: (message) => notifyRuntimeTransientRef.current(message),
    refreshSessionsRef,
    onRestoreSession: (sessionId) => handleSelectSessionRef.current(sessionId),
    onClearActiveSession: () => handleNewSessionRef.current(),
  });

  const {
    streamingThreadIds,
    setStreamingThreadIds,
    pendingComposerStream,
    setPendingComposerStream,
    streaming,
    streamControllersRef,
    threadTurnRef,
    streamSessionRef,
    abortThreadStream,
    handleCancelStream,
  } = useTurnStream({
    resumedThreadId,
    streamingRef,
    t,
    onCancelSideEffects: () => {
      setApprovalRef.current(null);
      setLastTurnOutputTokensRef.current(null);
    },
  });

  const {
    runtimeConn,
    runtimeSessionEstablished,
    setRuntimeSessionEstablished,
    runtimeReachability,
    reconcileRuntimeAfterFetchFailure,
    notifyRuntimeTransient,
  } = useRuntimeConnection({ streaming, streamingRef, t, refreshSessionsRef });

  const {
    approval,
    setApproval,
    approvalBusy,
    approvalPolicy,
    autoApprove,
    setAutoApprove,
    handleAutoApproveChange,
    syncAutoApproveFromRunMode,
    handleSystemSettingsSaved,
    handleApproveDecision,
    showApprovalIfOwned,
    clearApproval,
  } = useTurnApproval({
    t,
    threadTurnRef,
    desktopHost,
    runModeRef,
  });

  const {
    agentStates,
    resetAgentPanel,
    onAgentSpawnToolStarted,
    onAgentSpawnToolCompleted,
    applyAgentStreamEvent,
    subagentActiveCount,
    narrativeSpawnSuspected,
  } = useAgentPanelState({ messages });

  setRuntimeSessionEstablishedRef.current = setRuntimeSessionEstablished;
  notifyRuntimeTransientRef.current = notifyRuntimeTransient;
  reconcileRuntimeAfterFetchFailureRef.current = reconcileRuntimeAfterFetchFailure;
  setApprovalRef.current = setApproval;
  setLastTurnOutputTokensRef.current = setLastTurnOutputTokens;

  const [panelPreview, setPanelPreview] = useState<PreviewState | null>(null);
  const [focusWorkspaceFilesNonce, setFocusWorkspaceFilesNonce] = useState(0);
  const [focusWorkspaceFilesRelPath, setFocusWorkspaceFilesRelPath] = useState<string | null>(
    null,
  );
  const [focusWorkspaceDiffNonce, setFocusWorkspaceDiffNonce] = useState(0);
  const [composerMentionNonce, setComposerMentionNonce] = useState(0);
  const [composerMentionRel, setComposerMentionRel] = useState<string | null>(null);
  const [composerMentionIsDir, setComposerMentionIsDir] = useState(false);
  const [modelParams, setModelParams] = useState<ModelParams>(() => loadModelParams());
  const [modelParamsOpen, setModelParamsOpen] = useState(false);
  const [editDraft, setEditDraft] = useState<{ messageId: string; content: string } | null>(null);
  const [backtrackDraft, setBacktrackDraft] = useState<{
    messageId: string;
    content: string;
    depthFromTail: number;
  } | null>(null);
  const [backtrackBusy, setBacktrackBusy] = useState(false);
  const [composerPrefill, setComposerPrefill] = useState<{ text: string; nonce: number } | undefined>();
  const [contextWindowTokens, setContextWindowTokens] = useState(DEFAULT_CONTEXT_WINDOW_TOKENS);
  /** Thread detail for empty-transcript fallback only (not summed for context %). */
  const [threadDetailForContext, setThreadDetailForContext] = useState<
    import('./lib/contextUsage').ThreadDetailWithTurns | null
  >(null);
  /** Last completed turn output tokens (Claude-style “↓ N tokens” hint). */
  /** Runtime-aligned context snapshot (TUI `estimate_input_tokens_conservative`). */
  const [threadContextSnapshot, setThreadContextSnapshot] =
    useState<ThreadContextSnapshot | null>(null);
  const threadContextSnapshotRef = useRef<ThreadContextSnapshot | null>(null);
  const threadContextCacheRef = useRef<Map<string, ThreadContextSnapshot>>(new Map());

  const applyThreadContextSnapshot = useCallback((threadId: string, snap: ThreadContextSnapshot) => {
    threadContextCacheRef.current.set(threadId, snap);
    if (resumedThreadIdRef.current !== threadId) {
      return;
    }
    setThreadContextSnapshot(snap);
    setContextWindowTokens(snap.context_window_tokens);
  }, []);

  const restoreThreadContextFromCache = useCallback((threadId: string) => {
    const cached = threadContextCacheRef.current.get(threadId);
    if (!cached || resumedThreadIdRef.current !== threadId) {
      return;
    }
    setThreadContextSnapshot(cached);
    setContextWindowTokens(cached.context_window_tokens);
  }, []);

  const refreshThreadContext = useCallback(
    async (threadId: string) => {
      try {
        const snap = await getThreadContext(threadId);
        applyThreadContextSnapshot(threadId, snap);
      } catch {
        if (resumedThreadIdRef.current !== threadId) {
          return;
        }
        restoreThreadContextFromCache(threadId);
      }
    },
    [applyThreadContextSnapshot, restoreThreadContextFromCache],
  );

  const contextUsedTokens = useMemo(
    () =>
      resolveContextUsedTokens(
        messages,
        threadDetailForContext,
        contextWindowTokens,
        threadContextSnapshot,
      ),
    [messages, threadDetailForContext, contextWindowTokens, threadContextSnapshot],
  );

  const contextUsagePct = useMemo(
    () => resolveContextUsagePercent(contextUsedTokens, contextWindowTokens, threadContextSnapshot),
    [contextUsedTokens, contextWindowTokens, threadContextSnapshot],
  );

  const [desktopApiKeyConfigured, setDesktopApiKeyConfigured] = useState<boolean | null>(null);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [rightPanelCollapsed, setRightPanelCollapsed] = useState(() =>
    loadStoredRightPanelCollapsed(),
  );
  const toggleDevtools = useCallback(() => {
    if (!desktopHost) return;
    void import('@tauri-apps/api/core').then(({ invoke }) =>
      invoke('plugin:webview|internal_toggle_devtools'),
    );
  }, [desktopHost]);

  useKeyboardShortcuts([
    { key: 'k', ctrl: true, description: t('keyboard.newSession'), handler: () => handleNewSession() },
    {
      key: 'n',
      ctrl: true,
      shift: true,
      global: true,
      description: t('keyboard.newWindow'),
      handler: () => {
        void createAgentWindow(selectedWorkspace).catch((e) => {
          toast.error((e as Error).message);
        });
      },
    },
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

  const selectSessionGenerationRef = useRef(0);
  const selectSessionAbortRef = useRef<AbortController | null>(null);
  /** Per-session UI snapshots so switching back restores tools + thinking without waiting on replay. */
  const sessionUiCacheRef = useRef<Map<string, CachedUiMessage[]>>(new Map());
  const messagesRef = useRef<Message[]>([]);
  /** User chose another inspector tab; do not yank them back to checklist on poll. */
  const suppressChecklistAutoSwitchRef = useRef(false);
  const suppressAuditAutoSwitchRef = useRef(false);

  const { handleSend, resetTurnPersistState } = useTurnSend({
    t,
    streaming,
    resumedThreadId,
    resumedThreadIdRef,
    runMode,
    autoApprove,
    routeIntent,
    selectedModel,
    selectedWorkspace,
    taskTypePreference,
    modelParams,
    desktopHost,
    streamControllersRef,
    threadTurnRef,
    streamSessionRef,
    setStreamingThreadIds,
    setPendingComposerStream,
    setMessages,
    setResumedThreadId,
    setActiveSessionId,
    setRuntimeSessionEstablished,
    setLastTurnOutputTokens,
    activeSessionIdRef,
    sessionUiCacheRef,
    refreshSessions,
    refreshThreadContext,
    applyThreadContextSnapshot,
    notifyRuntimeTransient,
    resetAgentPanel,
    onAgentSpawnToolStarted,
    onAgentSpawnToolCompleted,
    applyAgentStreamEvent,
    showApprovalIfOwned,
  });

  useEffect(() => {
    threadContextSnapshotRef.current = threadContextSnapshot;
  }, [threadContextSnapshot]);

  useEffect(() => {
    messagesRef.current = messages;
  }, [messages]);

  useEffect(() => {
    suppressChecklistAutoSwitchRef.current = false;
    suppressAuditAutoSwitchRef.current = false;
  }, [resumedThreadId]);

  const { taskActivity, agentActivity, checklistActivity, acknowledgeInspectorView } =
    useInspectorUnread({
      agentStates,
      resumedThreadId,
      activeInspector,
      runtimeSessionEstablished,
      streaming,
    });

  const handleInspectorChange = useCallback(
    (view: RightPanelView) => {
      if (activeInspector === 'checklist' && view !== 'checklist') {
        suppressChecklistAutoSwitchRef.current = true;
      }
      if (view === 'checklist') {
        suppressChecklistAutoSwitchRef.current = false;
      }
      if (activeInspector === 'audit' && view !== 'audit') {
        suppressAuditAutoSwitchRef.current = true;
      }
      if (view === 'audit') {
        suppressAuditAutoSwitchRef.current = false;
      }
      acknowledgeInspectorView(view);
      setActiveInspector(view);
      setRightPanelCollapsed(false);
    },
    [activeInspector, acknowledgeInspectorView],
  );

  const handleRequestChecklist = useCallback(() => {
    if (suppressChecklistAutoSwitchRef.current) {
      return;
    }
    setRightPanelCollapsed(false);
    setActiveInspector('checklist');
  }, []);

  const handleRequestAudit = useCallback(() => {
    if (suppressAuditAutoSwitchRef.current) {
      return;
    }
    setRightPanelCollapsed(false);
    setActiveInspector('audit');
  }, []);

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
      localStorage.setItem(workspaceStorageKey(windowLabel), ws);
    } catch {
      /* ignore */
    }
    void updateWindowTitle(ws);
  }, [selectedWorkspace, windowLabel]);

  useEffect(() => {
    void (async () => {
      const { label, primaryWorkspace } = await initWindowContext();
      setWindowLabel(label);
      if (primaryWorkspace.trim()) {
        setSelectedWorkspace(primaryWorkspace);
        return;
      }
      const prefs = loadComposerPrefs(label);
      if (prefs.workspace) {
        setSelectedWorkspace(prefs.workspace);
      }
    })();
  }, []);

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
    if (!resumedThreadId) {
      setThreadContextSnapshot(null);
      return;
    }
    restoreThreadContextFromCache(resumedThreadId);
    if (!streaming) {
      void refreshThreadContext(resumedThreadId);
      const id = window.setInterval(
        () => void refreshThreadContext(resumedThreadId),
        THREAD_CONTEXT_POLL_STREAMING_MS,
      );
      return () => window.clearInterval(id);
    }
    // C-channel: context updates ride `panel.context` on the live SSE stream.
    return undefined;
  }, [resumedThreadId, streaming, refreshThreadContext, restoreThreadContextFromCache]);

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

  useEffect(() => {
    try {
      localStorage.setItem(
        RIGHT_PANEL_COLLAPSED_STORAGE_KEY,
        rightPanelCollapsed ? 'true' : 'false',
      );
    } catch {
      /* ignore */
    }
  }, [rightPanelCollapsed]);

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

  /** Sidecar restart (e.g. save system settings) kills in-flight SSE — clear stale「生成中」UI. */
  const abortActiveStreamForSidecarRestart = useCallback(() => {
    if (!streamingRef.current) return;
    for (const c of streamControllersRef.current.values()) {
      c.abort();
    }
    streamControllersRef.current.clear();
    const label = t('composer.runtimeSidecarRestart');
    setMessages((prev) =>
      prev.map((m) => {
        if (!m.isStreaming) return m;
        const tools = (m.tools ?? []).map((tool) =>
          tool.status === 'running' ? { ...tool, status: 'error' as const } : tool,
        );
        const trimmed = m.content.trim();
        let content = m.content;
        if (!trimmed) {
          content = label;
        } else if (!trimmed.includes(label)) {
          content = `[${label}] ${m.content}`;
        }
        return { ...m, tools, content, isStreaming: false };
      }),
    );
    const session = streamSessionRef.current;
    if (session) {
      session.finishOnce();
    } else {
      setStreamingThreadIds(new Set());
      setPendingComposerStream(false);
    }
    notifyRuntimeTransient(t('banner.runtimeRestartDuringStream'));
  }, [t, notifyRuntimeTransient]);

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
          localStorage.getItem(workspaceStorageKey(getWindowLabel()))?.trim() ??
            selectedWorkspace,
          setSelectedWorkspace,
        );
      } catch {
        setDesktopHost(false);
        setDesktopApiKeyConfigured(null);
      }
    })();
  }, []);

  useEffect(() => {
    runModeRef.current = runMode;
  }, [runMode]);

  const handleRunModeChange = useCallback(
    (mode: DesktopRunModeId) => {
      setRunMode(mode);
      runModeRef.current = mode;
      syncAutoApproveFromRunMode(mode);
    },
    [syncAutoApproveFromRunMode],
  );

  useEffect(() => {
    refreshApiKeyStatus();
  }, [refreshApiKeyStatus]);

  // ── Startup gate: windows start invisible; show when sidecar is ready ──
  useEffect(() => {
    if (!desktopHost) return;
    let cancelled = false;
    let timedOut = false;
    let unlistenReady: (() => void) | undefined;

    const showWindow = () => {
      void import('@tauri-apps/api/window')
        .then(({ getCurrentWindow }) => getCurrentWindow().show())
        .catch(() => {});
    };

    const onReady = () => {
      if (cancelled) return;
      void refreshSessions();
      showWindow();
    };

    const fallback = setTimeout(() => {
      timedOut = true;
      showWindow();
    }, 5000);

    // Second+ windows open after boot never receive the global `sidecar://ready` broadcast.
    void waitForRuntimeBootReady({ timeoutMs: 2_000, intervalMs: 100 }).then((ready) => {
      if (cancelled || !ready) return;
      clearTimeout(fallback);
      if (!timedOut) {
        onReady();
      }
    });

    void import('@tauri-apps/api/event')
      .then(({ listen }) =>
        listen<Record<string, unknown>>('sidecar://ready', () => {
          clearTimeout(fallback);
          if (!timedOut) {
            onReady();
          }
        }),
      )
      .then((fn) => {
        unlistenReady = fn;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      clearTimeout(fallback);
      unlistenReady?.();
    };
  }, [desktopHost, refreshSessions]);

  useEffect(() => {
    if (!desktopHost) return;
    let unlisten: (() => void) | undefined;
    void import('@tauri-apps/api/event')
      .then(({ listen }) =>
        listen('sidecar://restarting', () => {
          abortActiveStreamForSidecarRestart();
        }),
      )
      .then((fn) => {
        unlisten = fn;
      })
      .catch(() => {});
    return () => {
      unlisten?.();
    };
  }, [desktopHost, abortActiveStreamForSidecarRestart]);

  const handleSelectSession = useCallback(
    async (sessionId: string) => {
      const gen = ++selectSessionGenerationRef.current;
      selectSessionAbortRef.current?.abort();
      const selectAbort = new AbortController();
      selectSessionAbortRef.current = selectAbort;

      const outgoingSessionId = activeSessionIdRef.current;
      if (outgoingSessionId && messagesRef.current.length > 0) {
        cacheSessionUiMessages(
          sessionUiCacheRef.current,
          outgoingSessionId,
          messagesRef.current,
        );
      }
      const outgoingThreadId = resumedThreadIdRef.current;
      const outgoingSnapshot = threadContextSnapshotRef.current;
      if (outgoingThreadId && outgoingSnapshot) {
        threadContextCacheRef.current.set(outgoingThreadId, outgoingSnapshot);
      }
      if (outgoingThreadId) {
        abortThreadStream(outgoingThreadId);
      }

      toast.dismissAll();
      setActiveSessionId(sessionId);
      setResumedThreadId(null);
      setThreadTrustMode(false);
      setPanelPreview(null);
      resetTurnPersistState();

      const cachedUi = getCachedSessionUiMessages(sessionUiCacheRef.current, sessionId);
      if (cachedUi?.length) {
        setMessages(cachedUi as Message[]);
        cacheSessionUiMessages(sessionUiCacheRef.current, sessionId, cachedUi);
      } else {
        setMessages([]);
      }

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
        if (!cachedUi?.length) {
          setMessages(sessionFallback);
        }
        resumedThreadIdRef.current = resumed.thread_id;
        setResumedThreadId(resumed.thread_id);
        setRuntimeSessionEstablished(true);
        restoreThreadContextFromCache(resumed.thread_id);
        try {
          const fromThread = await rebuildMessagesFromThreadEvents(resumed.thread_id, {
            signal: selectAbort.signal,
          });
          if (gen !== selectSessionGenerationRef.current) {
            return;
          }
          if (fromThread.length > 0) {
            const rebuilt = fromThread as Message[];
            setMessages(rebuilt);
            cacheSessionUiMessages(sessionUiCacheRef.current, sessionId, rebuilt);
          } else if (!cachedUi?.length && sessionFallback.length > 0) {
            cacheSessionUiMessages(sessionUiCacheRef.current, sessionId, sessionFallback);
          }
        } catch {
          /* runtime replay failed — keep in-memory cache or session text fallback */
          if (!cachedUi?.length && sessionFallback.length > 0) {
            setMessages(sessionFallback);
          }
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
          void registerWindowThread(resumed.thread_id);
          if (gen === selectSessionGenerationRef.current) {
            void refreshThreadContext(resumed.thread_id);
          }
        } catch (syncErr) {
          if (gen !== selectSessionGenerationRef.current) {
            return;
          }
          setThreadDetailForContext(null);
          setContextWindowTokens(contextWindowTokensForModel(selectedModel));
          const errMsg = syncErr instanceof Error ? syncErr.message : String(syncErr);
          notifyRuntimeTransient(t('banner.threadWorkspaceError', { errMsg }));
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
          notifyRuntimeTransient(t('banner.unauthorized401'));
        } else {
          toast.error(t('banner.loadSessionFailed', { message: err.message }));
        }
        reconcileRuntimeAfterFetchFailure();
      }
    },
    [
      reconcileRuntimeAfterFetchFailure,
      notifyRuntimeTransient,
      refreshThreadContext,
      restoreThreadContextFromCache,
      selectedModel,
      abortThreadStream,
      resetTurnPersistState,
      t,
    ],
  );

  handleSelectSessionRef.current = handleSelectSession;

  const handleNewSession = useCallback(() => {
    abortThreadStream(resumedThreadIdRef.current);
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
    resetTurnPersistState();
    clearApproval();
  }, [abortThreadStream, clearApproval, resetTurnPersistState, selectedModel]);

  handleNewSessionRef.current = handleNewSession;

  useEffect(() => {
    if (!officeSession) return;
    if (
      activeInspector === 'agents' ||
      activeInspector === 'index' ||
      activeInspector === 'checklist' ||
      activeInspector === 'audit' ||
      activeInspector === 'routing'
    ) {
      setActiveInspector('workspace');
    }
  }, [officeSession, activeInspector]);

  /** Office composer uses Documents/Zagens when not bound to a resumed thread workspace. */
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
          toast.warning(t('banner.activeTurnBlocking'));
        } else {
          toast.error(t('banner.updateThreadWorkspace', { msg }));
        }
        throw err;
      }
    },
    [resumedThreadId, t],
  );

  const closePanelPreview = useCallback(() => {
    setPanelPreview(null);
  }, []);

  const addWorkspaceFileToChat = useCallback((relPath: string, isDirectory = false) => {
    const rel = normalizeWorkspaceRelPath(relPath);
    if (!rel) return;
    setComposerMentionRel(rel);
    setComposerMentionIsDir(isDirectory);
    setComposerMentionNonce((n) => n + 1);
  }, []);

  const revealWorkspaceFileInDirectory = useCallback((relPath: string) => {
    const rel = normalizeWorkspaceRelPath(relPath);
    if (!rel) return;
    setActiveInspector('workspace');
    setFocusWorkspaceFilesRelPath(rel);
    setFocusWorkspaceFilesNonce((n) => n + 1);
  }, []);

  const openWorkspaceFileForPreview = useCallback(
    async (relPath: string, title?: string) => {
      if (!isRuntimeApiAvailable(runtimeConn, runtimeReachability)) {
        throw new Error(t('banner.runtimeNotConnected'));
      }
      revealWorkspaceFileInDirectory(relPath);
      const state = await loadWorkspaceFileIntoPreview({
        relPath,
        title,
        workspaceRoot: selectedWorkspace,
        resumedThreadId,
        desktopHost,
      });
      setPanelPreview(state);
    },
    [runtimeConn, runtimeReachability, selectedWorkspace, resumedThreadId, desktopHost, t, revealWorkspaceFileInDirectory],
  );

  const handleChatOpenWorkspacePath = useCallback(
    async (relPath: string) => {
      try {
        await openWorkspaceFileForPreview(relPath);
      } catch (e) {
        toast.error(t('banner.openFileFailed', { err: formatWorkspaceFileError(e, t) }));
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
      toast.warning(t('banner.exportNoSession'));
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
        toast.error(t('banner.exportNoData'));
      }
    }
  }, [activeSessionId, t]);

  const handleExportThreadJson = useCallback(async () => {
    if (!resumedThreadId) {
      toast.warning(t('banner.exportThreadNoId'));
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
        toast.error(t('banner.exportThreadNoData'));
      }
    }
  }, [resumedThreadId, t]);

  const handleEditMessage = useCallback(
    (messageId: string, content: string) => {
      if (streaming || !resumedThreadId) {
        toast.warning(t('chat.editNeedsThread'));
        return;
      }
      const userMsgs = messages.filter((m) => m.role === 'user');
      const lastUser = userMsgs[userMsgs.length - 1];
      if (!lastUser || lastUser.id !== messageId) {
        toast.warning(t('chat.editLastOnly'));
        return;
      }
      setEditDraft({ messageId, content });
    },
    [streaming, resumedThreadId, messages, t],
  );

  const handleConfirmEdit = useCallback(() => {
    if (!editDraft?.content.trim()) {
      setEditDraft(null);
      return;
    }
    const draft = editDraft;
    setEditDraft(null);
    handleSend(
      { displayContent: draft.content.trim(), apiPrompt: draft.content.trim() },
      { editFromMessageId: draft.messageId },
    );
  }, [editDraft, handleSend]);

  const handleBacktrackFromMessage = useCallback(
    (messageId: string, content: string) => {
      if (streaming || !resumedThreadId) {
        toast.warning(t('chat.backtrackNeedsThread'));
        return;
      }
      const depth = depthFromTailForUserMessage(messages, messageId);
      if (depth == null) {
        return;
      }
      setBacktrackDraft({ messageId, content, depthFromTail: depth });
    },
    [streaming, resumedThreadId, messages, t],
  );

  const handleConfirmBacktrack = useCallback(async () => {
    if (!backtrackDraft || !resumedThreadId || backtrackBusy) {
      return;
    }
    const sourceThreadId = resumedThreadId;
    const draft = backtrackDraft;
    setBacktrackDraft(null);
    setBacktrackBusy(true);
    try {
      const { thread, original_user_text } = await forkThreadAtUserMessage(
        sourceThreadId,
        draft.depthFromTail,
      );
      const newThreadId = thread.id;
      streamControllersRef.current.get(sourceThreadId)?.abort();
      streamControllersRef.current.delete(sourceThreadId);
      setStreamingThreadIds(new Set());
      setPendingComposerStream(false);
      resetAgentPanel();
      resumedThreadIdRef.current = newThreadId;
      setResumedThreadId(newThreadId);
      threadTurnRef.current = { threadId: newThreadId, turnId: '' };
      resetTurnPersistState();

      const rebuilt = (await rebuildMessagesFromThreadEvents(newThreadId)) as Message[];
      setMessages(rebuilt);
      if (activeSessionIdRef.current) {
        cacheSessionUiMessages(sessionUiCacheRef.current, activeSessionIdRef.current, rebuilt);
      }

      const threadDetail = await getThreadDetail(newThreadId);
      setThreadDetailForContext(threadDetail);
      const turns = threadDetail.turns ?? [];
      const lastTurn = turns.length > 0 ? turns[turns.length - 1] : undefined;
      const lastOut = lastTurn?.usage?.output_tokens;
      setLastTurnOutputTokens(
        lastOut != null && Number.isFinite(lastOut) && lastOut > 0 ? lastOut : null,
      );
      void refreshThreadContext(newThreadId);

      const prefill = original_user_text?.trim();
      if (prefill) {
        setComposerPrefill({ text: prefill, nonce: Date.now() });
      }
      toast.success(t('chat.backtrackSuccess'));
    } catch (e) {
      toast.error(t('chat.backtrackFailed', { message: (e as Error).message }));
    } finally {
      setBacktrackBusy(false);
    }
  }, [backtrackDraft, resumedThreadId, backtrackBusy, refreshThreadContext, resetAgentPanel, resetTurnPersistState, t]);

  const auditActivity = useAuditNavActivity({
    threadId: resumedThreadId,
    activeInspector,
    streaming,
    runtimeSessionEstablished,
    narrativeSpawnSuspected,
  });

  return (
    <div className="flex flex-col h-screen w-screen bg-canvas">
      <SkipToMainLink />
      <TitleBar
        desktopHost={desktopHost}
        onNewWindow={() => {
          void createAgentWindow(selectedWorkspace).catch((e) => {
            toast.error((e as Error).message);
          });
        }}
      />
      <div className="flex flex-1 min-h-0 bg-canvas">
      <ApprovalDialog
        open={approval != null}
        toolName={approval?.toolName ?? ''}
        description={approval?.description ?? ''}
        busy={approvalBusy}
        onApprove={() => void handleApproveDecision('approve')}
        onDeny={() => void handleApproveDecision('deny')}
      />
      <ModelParamsDialog
        open={modelParamsOpen}
        initial={modelParams}
        onClose={() => setModelParamsOpen(false)}
        onApply={(params) => {
          setModelParams(params);
          saveModelParams(params);
          setModelParamsOpen(false);
        }}
      />
      {editDraft ? (
        <div
          className="fixed inset-0 z-[10050] flex items-center justify-center bg-overlay"
          onClick={(e) => {
            if (e.target === e.currentTarget) setEditDraft(null);
          }}
        >
          <div
            className="w-full max-w-lg rounded-2xl border border-card-border bg-card p-5 shadow-lg"
            role="dialog"
            aria-modal="true"
            aria-labelledby="edit-message-title"
          >
            <h3 id="edit-message-title" className="mb-3 text-base font-semibold text-t-text">
              {t('chat.editTitle')}
            </h3>
            <textarea
              className="min-h-[120px] w-full resize-y rounded-lg border border-input-border bg-input-bg px-3 py-2 text-sm text-t-text outline-none focus:border-accent"
              value={editDraft.content}
              onChange={(e) => setEditDraft((d) => (d ? { ...d, content: e.target.value } : d))}
              autoFocus
            />
            <div className="mt-4 flex justify-end gap-2">
              <button
                type="button"
                className="rounded-lg px-4 py-2 text-sm text-t-text-secondary hover:bg-hover"
                onClick={() => setEditDraft(null)}
              >
                {t('modelParams.cancel')}
              </button>
              <button
                type="button"
                className="rounded-lg bg-accent px-4 py-2 text-sm font-medium text-accent-text hover:opacity-90"
                onClick={handleConfirmEdit}
              >
                {t('chat.editSubmit')}
              </button>
            </div>
          </div>
        </div>
      ) : null}
      {backtrackDraft ? (
        <div
          className="fixed inset-0 z-[10050] flex items-center justify-center bg-overlay"
          onClick={(e) => {
            if (e.target === e.currentTarget && !backtrackBusy) setBacktrackDraft(null);
          }}
        >
          <div
            className="w-full max-w-lg rounded-2xl border border-card-border bg-card p-5 shadow-lg"
            role="dialog"
            aria-modal="true"
            aria-labelledby="backtrack-message-title"
          >
            <h3 id="backtrack-message-title" className="mb-2 text-base font-semibold text-t-text">
              {t('chat.backtrackTitle')}
            </h3>
            <p className="mb-3 text-sm text-t-text-secondary">{t('chat.backtrackBody')}</p>
            <div className="mb-4 rounded-lg border border-card-border bg-canvas-alt px-3 py-2 text-sm text-t-text-secondary line-clamp-4 whitespace-pre-wrap">
              {backtrackDraft.content}
            </div>
            <div className="flex justify-end gap-2">
              <button
                type="button"
                className="rounded-lg px-4 py-2 text-sm text-t-text-secondary hover:bg-hover disabled:opacity-50"
                disabled={backtrackBusy}
                onClick={() => setBacktrackDraft(null)}
              >
                {t('modelParams.cancel')}
              </button>
              <button
                type="button"
                className="rounded-lg bg-accent px-4 py-2 text-sm font-medium text-accent-text hover:opacity-90 disabled:opacity-50"
                disabled={backtrackBusy}
                onClick={() => void handleConfirmBacktrack()}
              >
                {backtrackBusy ? t('chat.backtrackWorking') : t('chat.backtrackConfirm')}
              </button>
            </div>
          </div>
        </div>
      ) : null}
      <Sidebar
        sessions={visibleSessions}
        showAllSessions={showAllSessions}
        onToggleShowAllSessions={() => setShowAllSessions((v) => !v)}
        activeSessionId={activeSessionId}
        onNewSession={handleNewSession}
        onSelectSession={handleSelectSession}
        onDeleteSession={handleDeleteSession}
        desktopHost={desktopHost}
        runtimeConn={runtimeConn}
        streaming={streaming}
        runtimeSessionEstablished={runtimeSessionEstablished}
        apiKeyConfigured={desktopApiKeyConfigured}
        activeInspector={activeInspector}
        onInspectorChange={handleInspectorChange}
        collapsed={sidebarCollapsed}
        onToggleCollapse={() => setSidebarCollapsed((v) => !v)}
        officeSession={officeSession}
        checklistActivity={checklistActivity}
        auditActivity={auditActivity}
        taskActivity={taskActivity}
        agentActivity={agentActivity}
      />
      {/* left toggle strip — visible when sidebar collapsed */}
      {sidebarCollapsed && (
        <button
          type="button"
          onClick={() => setSidebarCollapsed(false)}
          className="chrome-seam-r shrink-0 w-8 bg-canvas hover:bg-hover transition-colors flex items-center justify-center group"
          title={t('sidebar.expand')}
          aria-label={t('sidebar.expand')}
        >
          <svg className="w-3.5 h-3.5 text-t-text-muted group-hover:text-t-text transition-colors" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden>
            <path d="M5 3.5v9" strokeLinecap="round" />
            <path d="M8 8l3-3v6l-3-3z" strokeLinejoin="round" />
          </svg>
        </button>
      )}
      <main
        id="main-content"
        tabIndex={-1}
        className="flex min-h-0 flex-1 flex-col min-w-0 bg-card outline-none"
      >
        {/* F3 — DOM order: composer before transcript so Tab reaches input after sidebar
            without traversing every message control; flex `order` keeps chat-on-top layout. */}
        <section
          className="order-2 shrink-0"
          aria-label={t('a11y.composerRegion')}
        >
        <Composer
          onSend={handleSend}
          onCancel={handleCancelStream}
          disabled={streaming}
          autoApprove={autoApprove}
          approvalPolicy={approvalPolicy}
          onAutoApproveChange={handleAutoApproveChange}
          runMode={runMode}
          onRunModeChange={handleRunModeChange}
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
          onOpenModelParams={() => setModelParamsOpen(true)}
          workspace={selectedWorkspace}
          onWorkspaceChange={handleComposerWorkspaceChange}
          resumedThreadActive={resumedThreadId != null && resumedThreadId.length > 0}
          contextUsagePct={contextUsagePct}
          contextUsedTokens={contextUsedTokens}
          contextWindowTokens={contextWindowTokens}
          contextSource={threadContextSnapshot?.source}
          compactionThresholdTokens={threadContextSnapshot?.compaction_threshold_tokens}
          lastApiInputTokens={threadContextSnapshot?.last_api_input_tokens ?? null}
          lastTurnOutputTokens={lastTurnOutputTokens}
          officeSession={officeSession}
          workspaceMention={
            composerMentionRel
              ? {
                  relPath: composerMentionRel,
                  isDirectory: composerMentionIsDir,
                  nonce: composerMentionNonce,
                }
              : undefined
          }
          composerPrefill={composerPrefill}
        />
        </section>
        <section
          className="order-1 flex min-h-0 min-w-0 flex-1 flex-col"
          aria-label={t('a11y.chatLog')}
        >
          <ChatView
            messages={messages}
            workspaceRoot={selectedWorkspace}
            desktopHost={desktopHost}
            agentStates={agentStates}
            onOpenWorkspacePath={handleChatOpenWorkspacePath}
            onRevealWorkspacePath={revealWorkspaceFileInDirectory}
            onOpenDiffInPanel={openDiffInPanel}
            onRetryMessage={(content) =>
              handleSend({ displayContent: content, apiPrompt: content })
            }
            onEditMessage={resumedThreadId ? handleEditMessage : undefined}
            onBacktrackFromMessage={
              resumedThreadId ? handleBacktrackFromMessage : undefined
            }
          />
        </section>
      </main>
      {/* right panel toggle strip */}
      {!rightPanelCollapsed && (
        <RightPanel
          view={activeInspector}
          officeSession={officeSession}
          desktopHost={desktopHost}
          runtimeConn={runtimeConn}
          runtimeSessionEstablished={runtimeSessionEstablished}
          apiKeyConfigured={desktopApiKeyConfigured}
          onSavedApiKey={() => {
            refreshApiKeyStatus();
            toast.dismissAll();
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
              toast.dismissAll();
            } catch (e) {
              const err = e as Error & { status?: number };
              toast.error(t('banner.trustModeFailed', { message: err.message }));
            }
          }}
          preview={panelPreview}
          onClosePreview={closePanelPreview}
          openWorkspaceFile={openWorkspaceFileForPreview}
          revealWorkspaceFile={revealWorkspaceFileInDirectory}
          addWorkspaceFileToChat={addWorkspaceFileToChat}
          focusFilesNonce={focusWorkspaceFilesNonce}
          focusFilesRelPath={focusWorkspaceFilesRelPath}
          focusDiffNonce={focusWorkspaceDiffNonce}
          agentStates={agentStates}
          onRequestChecklist={handleRequestChecklist}
          onRequestAudit={handleRequestAudit}
          subagentActiveCount={subagentActiveCount}
          narrativeSpawnSuspected={narrativeSpawnSuspected}
          streaming={streaming}
          messages={messages}
          onRequestMermaid={() => setActiveInspector('mermaid')}
          onRequestDiff={handleRequestDiffPanel}
          onCollapse={() => setRightPanelCollapsed(true)}
          onSystemSettingsSaved={handleSystemSettingsSaved}
          routeIntent={routeIntent}
          onRouteIntentChange={setRouteIntent}
        />
      )}
      {rightPanelCollapsed && (
        <button
          type="button"
          onClick={() => setRightPanelCollapsed(false)}
          className="chrome-seam-l shrink-0 w-8 bg-canvas hover:bg-hover transition-colors flex items-center justify-center group"
          title={t('rightPanel.expand')}
          aria-label={t('rightPanel.expand')}
        >
          <svg className="w-3.5 h-3.5 text-t-text-muted group-hover:text-t-text transition-colors" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden>
            <path d="M11 3.5v9" strokeLinecap="round" />
            <path d="M8 8L5 5v6l3-3z" strokeLinejoin="round" />
          </svg>
        </button>
      )}
    </div>
      </div>
  );
}

function TitleBar({
  desktopHost,
  onNewWindow,
}: {
  desktopHost: boolean;
  onNewWindow: () => void;
}) {
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
    if (desktopHost) {
      void import('./lib/windowBridge').then(({ closeCurrentWindow }) => closeCurrentWindow());
      return;
    }
    void import('@tauri-apps/api/window').then(({ getCurrentWindow }) => getCurrentWindow().hide());
  };

  return (
    <div
      data-tauri-drag-region
      className="flex items-center h-9 shrink-0 bg-canvas select-none"
    >
      <div className="flex items-center gap-0.5 shrink-0 pl-2" data-tauri-drag-region="false">
        {desktopHost && (
          <button
            type="button"
            onClick={onNewWindow}
            className="px-2 py-1 text-xs text-t-text-muted hover:text-t-text hover:bg-hover rounded transition-colors"
            title={t('titlebar.newWindow')}
          >
            {t('titlebar.newWindow')}
          </button>
        )}
      </div>
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