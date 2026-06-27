import { useCallback, useEffect, useMemo, useRef, useState, type Dispatch, type SetStateAction } from 'react';
import { useInspectorUnread } from './lib/useInspectorUnread';
import {
  getThreadDetail,
  patchThread,
} from './api/client';
import { useT } from './i18n';
import AppShell from './components/AppShell';
import OnboardingOverlay from './components/OnboardingOverlay';
import StartupConnectOverlay from './components/StartupConnectOverlay';
import { useAuditNavActivity } from './lib/useAuditNavActivity';
import { useHarnessGridData } from './lib/useHarnessGridData';
import { readSessionStripOpen, writeSessionStripOpen } from './hooks/useSessionStrip';
import { type ModelParams } from './components/ModelParamsDialog';
import {
  loadModelParams,
  saveModelParams,
} from './lib/modelParams';
import { type RightPanelView } from './components/RightPanel';
import {
  createAgentWindow,
  initWindowContext,
  updateWindowTitle,
  getWindowLabel,
  workspaceStorageKey,
} from './lib/windowBridge';
import useKeyboardShortcuts from './hooks/useKeyboardShortcuts';
import { usePreventBrowserReload } from './hooks/usePreventBrowserReload';
import { useRuntimeConnection } from './hooks/useRuntimeConnection';
import { useAgentPanelState } from './hooks/useAgentPanelState';
import { useChatMessageActions } from './hooks/useChatMessageActions';
import { useTraceExport } from './hooks/useTraceExport';
import { useDesktopShell } from './hooks/useDesktopShell';
import { useDeepLinkOpen } from './hooks/useDeepLinkOpen';
import { useStoragePressure } from './hooks/useStoragePressure';
import ShellLoadFailure from './components/ShellLoadFailure';
import { useSessionNavigation } from './hooks/useSessionNavigation';
import { useThreadContext } from './hooks/useThreadContext';
import { useWorkspacePanel } from './hooks/useWorkspacePanel';
import { useTurnSession } from './hooks/useTurnSession';
import { useTurnApproval, type ApprovalState } from './hooks/useTurnApproval';
import { useTurnStream } from './hooks/useTurnStream';
import { useTurnSend, type TurnChatMessage } from './hooks/useTurnSend';
import { useThreadStatusGlobalStream } from './hooks/useThreadStatusGlobalStream';
import { useStreamContextRegistry, createSetMessagesForView } from './hooks/useStreamContextRegistry';
import {
  collectStreamingSessionIds,
} from './lib/chat/streamContextStore';
import { evictIdleContextMessages } from './lib/chat/streamContextAccess';
import {
  getActiveThreadIdsFromStore,
  subscribeThreadStatusStore,
} from './lib/chat/threadStatusStore';
import { parseWriteOfficeOutputPath } from './lib/officeDeliverable';
import {
  type ComposerModelId,
  type DesktopRouteIntentOption,
  type DesktopRunModeId,
  type DesktopTaskTypePreference,
  type DesktopTaskTypeResolved,
  parseDesktopTaskTypeResolved,
} from './types/desktop';
import {
  mergeComposerModelOptions,
} from './lib/composerModels';
import { fetchSystemSettings, type SystemSettings } from './api/client';
import {
  applyOfficeDefaultWorkspace,
} from './lib/defaultWorkspace';
import { worktreeSessionLabel } from './lib/worktreePath';
import {
  ACTIVE_INSPECTOR_STORAGE_KEY,
  applyTheme,
  ensureDefaultComposerWorkspace,
  loadComposerPrefs,
  persistUseWorktreePreference,
  loadRouteIntentPreference,
  loadRunModePreference,
  loadStoredInspector,
  loadStoredRightPanelCollapsed,
  loadTaskTypePreference,
  syncTaskTypePreferencePersist,
  loadTheme,
  RIGHT_PANEL_COLLAPSED_STORAGE_KEY,
  ROUTE_INTENT_STORAGE_KEY,
  type Theme,
} from './lib/appPreferences';
import { type CachedUiMessage } from './lib/chat/sessionUiCache';
import { confirmDialog } from './lib/confirmDialog';
import { toast } from './lib/toast';
import type { LhtChipState } from './lib/lhtChip';
import { coerceRunModeForSession, isOfficeSession } from './lib/taskTypeSession';

export default function App() {
  const { t } = useT();
  usePreventBrowserReload();
  const [theme, setTheme] = useState<Theme>(loadTheme);
  const [windowLabel, setWindowLabel] = useState('dev');
  const [showAllSessions, setShowAllSessions] = useState(false);
  const [selectedModel, setSelectedModel] = useState<ComposerModelId>(() => loadComposerPrefs(getWindowLabel()).model);
  const [configuredModels, setConfiguredModels] = useState<string[]>([]);
  const composerModelOptions = mergeComposerModelOptions(configuredModels, selectedModel);
  const [selectedWorkspace, setSelectedWorkspace] = useState(() => loadComposerPrefs(getWindowLabel()).workspace);
  const [useWorktree, setUseWorktree] = useState(() => loadComposerPrefs(getWindowLabel()).useWorktree);
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
  const lockedThreadTaskTypeRef = useRef<DesktopTaskTypeResolved | null>(null);
  const [routeIntent, setRouteIntent] = useState<DesktopRouteIntentOption>(() => loadRouteIntentPreference());
  const [startupOverlayOpen, setStartupOverlayOpen] = useState(false);
  const [bootMainEntered, setBootMainEntered] = useState(false);

  const refreshSessionsRef = useRef<() => Promise<void>>(async () => {});
  const setRuntimeSessionEstablishedRef = useRef<Dispatch<SetStateAction<boolean>>>(() => {});
  const notifyRuntimeTransientRef = useRef<(message: string) => void>(() => {});
  const reconcileRuntimeAfterFetchFailureRef = useRef<() => void>(() => {});
  const handleSelectSessionRef = useRef<(sessionId: string) => void>(() => {});
  const handleNewSessionRef = useRef<() => void>(() => {});
  const streamingRef = useRef(false);
  const cancelCleanupRef = useRef<(() => void) | null>(null);
  const setApprovalRef = useRef<(value: ApprovalState | null) => void>(() => {});
  const setLastTurnOutputTokensRef = useRef<(value: number | null) => void>(() => {});
  const messagesRef = useRef<TurnChatMessage[]>([]);
  const sessionUiCacheRef = useRef<Map<string, CachedUiMessage[]>>(new Map());
  const suppressChecklistAutoSwitchRef = useRef(false);
  const suppressAuditAutoSwitchRef = useRef(false);

  const [lastTurnOutputTokens, setLastTurnOutputTokens] = useState<number | null>(null);
  const [lastCacheHitPercent, setLastCacheHitPercent] = useState<number | null>(null);
  const [lhtChip, setLhtChip] = useState<LhtChipState | null>(null);

  const streamingThreadIdsRef = useRef<Set<string>>(new Set());
  const resolveThreadSessionIdRef = useRef<(threadId: string) => string | null | undefined>(
    () => null,
  );
  const bindThreadSessionRef = useRef<
    (threadId: string, sessionId: string | null | undefined) => void
  >(() => {});
  const resolveThreadSessionIdForCheckpoint = useCallback(
    (threadId: string) => resolveThreadSessionIdRef.current(threadId),
    [],
  );
  const bindThreadSessionForCheckpoint = useCallback(
    (threadId: string, sessionId: string | null | undefined) =>
      bindThreadSessionRef.current(threadId, sessionId),
    [],
  );

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
    streamingThreadIdsRef,
    resolveThreadSessionId: resolveThreadSessionIdForCheckpoint,
    bindThreadSession: bindThreadSessionForCheckpoint,
    setRuntimeSessionEstablished: (value) => {
      setRuntimeSessionEstablishedRef.current(value);
    },
    reconcileRuntimeAfterFetchFailure: () => reconcileRuntimeAfterFetchFailureRef.current(),
    notifyRuntimeTransient: (message) => notifyRuntimeTransientRef.current(message),
    refreshSessionsRef,
    onRestoreSession: (sessionId) => handleSelectSessionRef.current(sessionId),
    onClearActiveSession: () => handleNewSessionRef.current(),
  });

  const onCancelStreamSideEffects = useCallback(() => {
    setApprovalRef.current(null);
    setLastTurnOutputTokensRef.current(null);
    setLastCacheHitPercent(null);
  }, []);

  // Multi-session P0.2: per-thread StreamContext registry (messages SSOT).
  const streamRegistry = useStreamContextRegistry();

  const {
    activeThreadIds,
    pendingComposerStream,
    setPendingComposerStream,
    streaming,
    streamControllersRef,
    pendingSendKeyRef,
    userStopRequestedRef,
    abortThreadStream,
    handleCancelStream,
  } = useTurnStream({
    resumedThreadId,
    streamingRef,
    streamRegistry,
    cancelCleanupRef,
    t,
    onCancelSideEffects: onCancelStreamSideEffects,
  });

  // Multi-session P0.4: ref mirror of store active threads for navigation detach/abort.
  useEffect(() => {
    const sync = () => {
      streamingThreadIdsRef.current = getActiveThreadIdsFromStore();
    };
    sync();
    return subscribeThreadStatusStore(sync);
  }, []);

  const getViewPointers = useCallback(
    () => ({
      threadId: resumedThreadIdRef.current ?? streamRegistry.activeThreadIdRef.current,
      sessionId: activeSessionIdRef.current,
    }),
    [streamRegistry],
  );

  const setMessagesForTurn = useMemo(
    () => createSetMessagesForView(streamRegistry, getViewPointers),
    [streamRegistry, getViewPointers],
  );

  const messages = useMemo(
    () => streamRegistry.getViewMessages(resumedThreadId, activeSessionId),
    [resumedThreadId, activeSessionId, streamRegistry.version, streamRegistry],
  );

  useEffect(() => {
    streamRegistry.setActiveThreadId(resumedThreadId);
  }, [resumedThreadId, streamRegistry]);

  // Promote session/new-session draft transcript when a runtime thread binds.
  useEffect(() => {
    const tid = resumedThreadId?.trim();
    if (!tid) return;
    streamRegistry.migrateDraftToThread(activeSessionId, tid);
  }, [resumedThreadId, activeSessionId, streamRegistry]);

  const bindThreadSession = useCallback(
    (threadId: string, sessionId: string | null | undefined) => {
      const tid = threadId.trim();
      const sid = sessionId?.trim();
      if (!tid || !sid) return;
      streamRegistry.ensureContext(tid, sid);
      streamRegistry.patchContext(tid, { sessionId: sid });
    },
    [streamRegistry],
  );

  resolveThreadSessionIdRef.current = (threadId: string) =>
    streamRegistry.getContext(threadId)?.sessionId ?? null;
  bindThreadSessionRef.current = bindThreadSession;

  const streamingSessionIds = useMemo(
    () =>
      collectStreamingSessionIds({
        activeThreadIds,
        contexts: streamRegistry.contexts,
        activeSessionId,
        resumedThreadId,
        activeThreadId: streamRegistry.activeThreadId,
        pendingComposerStream,
      }),
    [
      activeThreadIds,
      streamRegistry.version,
      streamRegistry.contexts,
      streamRegistry.activeThreadId,
      activeSessionId,
      resumedThreadId,
      pendingComposerStream,
    ],
  );

  // Evict idle non-active context payloads to cap registry memory (S0.2).
  useEffect(() => {
    const activeId =
      streamRegistry.activeThreadId ??
      resumedThreadId ??
      streamRegistry.activeThreadIdRef.current ??
      null;
    for (const tid of streamRegistry.contexts.keys()) {
      evictIdleContextMessages(
        streamRegistry,
        tid,
        activeId,
        sessionUiCacheRef.current,
      );
    }
  }, [streamRegistry.version, streamRegistry, resumedThreadId]);

  const {
    runtimeConn,
    runtimeSessionEstablished,
    setRuntimeSessionEstablished,
    runtimeReachability,
    reconcileRuntimeAfterFetchFailure,
    notifyRuntimeTransient,
    retryConnect,
    dismissRuntimeTransient,
  } = useRuntimeConnection({ streaming, streamingRef, t, refreshSessionsRef });

  useThreadStatusGlobalStream(runtimeConn);

  const {
    contextWindowTokens,
    setContextWindowTokens,
    threadDetailForContext,
    setThreadDetailForContext,
    threadContextSnapshot,
    threadContextSnapshotRef,
    threadContextCacheRef,
    applyThreadContextSnapshot,
    restoreThreadContextFromCache,
    refreshThreadContext,
    contextUsedTokens,
    contextUsagePct,
  } = useThreadContext({
    messages,
    resumedThreadId,
    resumedThreadIdRef,
    streaming,
  });

  const activeWorktreeName = useMemo(() => {
    const thread = threadDetailForContext?.thread;
    const named = thread?.worktree_name?.trim();
    if (named) return named;
    return worktreeSessionLabel(thread?.workspace ?? null);
  }, [threadDetailForContext]);

  const {
    desktopHost,
    shellInitFailed,
    shellPrefsReady,
    onboardingComplete,
    desktopApiKeyConfigured,
    platform,
    refreshApiKeyStatus,
    markOnboardingComplete,
  } = useDesktopShell({
    t,
    selectedWorkspace,
    setSelectedWorkspace,
    setTaskTypePreference,
  });

  const { snapshot: storageSnapshot, pauseTurns: storagePauseTurns, level: storageLevel } =
    useStoragePressure({
      desktopHost,
      workspaceRoot: selectedWorkspace,
      streaming,
      streamRegistry,
      resumedThreadId,
      handleCancelStream,
      t,
    });

  useEffect(() => {
    if (!desktopHost || !shellPrefsReady) return;
    const needsSetup =
      !onboardingComplete || desktopApiKeyConfigured === false;
    setStartupOverlayOpen(needsSetup);
  }, [desktopHost, shellPrefsReady, onboardingComplete, desktopApiKeyConfigured]);

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
    streamRegistry,
    resumedThreadIdRef,
    desktopHost,
    runModeRef,
  });

  const handleSystemSettingsSavedWithModels = useCallback(
    (settings: SystemSettings) => {
      handleSystemSettingsSaved(settings);
      setConfiguredModels(settings.available_models ?? []);
      const next = settings.default_model.trim();
      if (next) {
        setSelectedModel(next);
      }
    },
    [handleSystemSettingsSaved],
  );

  const syncComposerFromConfig = useCallback(() => {
    void fetchSystemSettings()
      .then((settings) => {
        setConfiguredModels(settings.available_models ?? []);
        const next = settings.default_model.trim();
        if (next) {
          setSelectedModel(next);
        }
      })
      .catch(() => {});
  }, []);

  const handleModelProvidersSaved = useCallback(() => {
    refreshApiKeyStatus();
    syncComposerFromConfig();
  }, [refreshApiKeyStatus, syncComposerFromConfig]);

  useEffect(() => {
    if (!desktopHost) return;
    let cancelled = false;
    fetchSystemSettings()
      .then((settings) => {
        if (cancelled) return;
        setConfiguredModels(settings.available_models ?? []);
        const next = settings.default_model.trim();
        if (next) {
          setSelectedModel(next);
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [desktopHost]);

  const {
    agentStates,
    resetAgentPanel,
    onAgentSpawnToolStarted,
    onAgentSpawnToolCompleted,
    applyAgentStreamEvent,
    subagentActiveCount,
    narrativeSpawnSuspected,
  } = useAgentPanelState({
    messages,
    resumedThreadId,
    workspaceRoot: selectedWorkspace,
    streaming,
    runtimeConn,
    runtimeSessionEstablished,
  });

  setRuntimeSessionEstablishedRef.current = setRuntimeSessionEstablished;
  notifyRuntimeTransientRef.current = notifyRuntimeTransient;
  reconcileRuntimeAfterFetchFailureRef.current = reconcileRuntimeAfterFetchFailure;
  setApprovalRef.current = setApproval;
  setLastTurnOutputTokensRef.current = setLastTurnOutputTokens;

  const [modelParams, setModelParams] = useState<ModelParams>(() => loadModelParams());
  const [modelParamsOpen, setModelParamsOpen] = useState(false);
  const [sessionStripOpen, setSessionStripOpen] = useState(() => readSessionStripOpen(false));
  const [rightPanelCollapsed, setRightPanelCollapsed] = useState(() =>
    loadStoredRightPanelCollapsed(),
  );
  const [highlightTaskId, setHighlightTaskId] = useState<string | null>(null);
  const [auditGridDismissed, setAuditGridDismissed] = useState(false);
  const [focusMode, setFocusMode] = useState(false);

  const {
    panelPreview,
    setPanelPreview,
    focusWorkspaceFilesNonce,
    focusWorkspaceFilesRelPath,
    focusWorkspaceDiffNonce,
    composerMentionNonce,
    composerMentionRel,
    composerMentionIsDir,
    composerPrefill,
    setComposerPrefill,
    closePanelPreview,
    addWorkspaceFileToChat,
    revealWorkspaceFileInDirectory,
    openWorkspaceFileForPreview,
    handleChatOpenWorkspacePath,
    openDiffInPanel,
    handleRequestDiffPanel,
    handleComposerWorkspaceChange,
    filesRefreshNonce,
    handleOfficeDeliverableReady,
  } = useWorkspacePanel({
    t,
    runtimeConn,
    runtimeReachability,
    selectedWorkspace,
    resumedThreadId,
    desktopHost,
    setSelectedWorkspace,
    setActiveInspector,
    setRightPanelCollapsed,
    setAuditGridDismissed,
  });

  useDeepLinkOpen({
    desktopHost,
    shellPrefsReady,
    setSelectedWorkspace,
    setTaskTypePreference,
    setUseWorktree,
    setComposerPrefill,
  });

  const { handleSend, resetTurnPersistState } = useTurnSend({
    t,
    runtimeConn,
    streaming,
    streamingRef,
    resumedThreadId,
    resumedThreadIdRef,
    runMode,
    autoApprove,
    routeIntent,
    selectedModel,
    selectedWorkspace,
    useWorktree,
    taskTypePreference,
    modelParams,
    desktopHost,
    streamControllersRef,
    pendingSendKeyRef,
    setPendingComposerStream,
    setMessages: setMessagesForTurn,
    setResumedThreadId,
    setActiveSessionId,
    setRuntimeSessionEstablished,
    setLastTurnOutputTokens,
    setLastCacheHitPercent,
    setLhtChip,
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
    cancelCleanupRef,
    userStopRequestedRef,
    handleCancelStream,
    storagePauseTurns,
    streamRegistry,
    bindThreadSession,
    onNavigateToSession: (sessionId) => handleSelectSessionRef.current(sessionId),
    onToolCompleted: (toolName, success, output) => {
      if (!officeSession || !success || toolName !== 'write_office') return;
      const rel = parseWriteOfficeOutputPath(output);
      if (rel) void handleOfficeDeliverableReady(rel);
    },
  });

  const {
    handleSelectSession,
    handleNewSession,
    handleOpenThreadById,
    sessionRestoreLoading,
    sessionRestoreSource,
    retrySessionRestore,
  } = useSessionNavigation({
    t,
    selectedModel,
    activeSessionIdRef,
    resumedThreadIdRef,
    streamRegistry,
    threadContextSnapshotRef,
    threadContextCacheRef,
    messagesRef,
    sessionUiCacheRef,
    abortThreadStream,
    streamingThreadIdsRef,
    streamControllersRef,
    bindThreadSession,
    desktopHost,
    setPendingComposerStream,
    showApprovalIfOwned,
    setLhtChip,
    applyThreadContextSnapshot,
    refreshSessions,
    resetTurnPersistState,
    clearApproval,
    setMessages: setMessagesForTurn,
    setActiveSessionId,
    setResumedThreadId,
    setRuntimeSessionEstablished,
    setThreadTrustMode,
    setPanelPreview,
    setThreadDetailForContext,
    setLastTurnOutputTokens,
    setLastCacheHitPercent,
    setContextWindowTokens,
    setSelectedWorkspace,
    setLockedThreadTaskType,
    refreshThreadContext,
    restoreThreadContextFromCache,
    reconcileRuntimeAfterFetchFailure,
    notifyRuntimeTransient,
    resetAgentPanel,
  });

  handleSelectSessionRef.current = handleSelectSession;

  const handleNewSessionPreserveMode = useCallback(() => {
    const locked = lockedThreadTaskTypeRef.current;
    if (locked != null) {
      setTaskTypePreference(locked);
    }
    handleNewSession();
  }, [handleNewSession]);

  handleNewSessionRef.current = handleNewSessionPreserveMode;

  const {
    editDraft,
    setEditDraft,
    backtrackDraft,
    setBacktrackDraft,
    backtrackBusy,
    handleExportSessionJson,
    handleExportThreadJson,
    handleEditMessage,
    handleConfirmEdit,
    handleBacktrackFromMessage,
    handleConfirmBacktrack,
  } = useChatMessageActions({
    t,
    streaming,
    resumedThreadId,
    activeSessionId,
    messages,
    activeSessionIdRef,
    resumedThreadIdRef,
    streamRegistry,
    streamControllersRef,
    sessionUiCacheRef,
    handleSend,
    setMessages: setMessagesForTurn,
    setResumedThreadId,
    setPendingComposerStream,
    setThreadDetailForContext,
    setLastTurnOutputTokens,
    setLastCacheHitPercent,
    setComposerPrefill,
    resetAgentPanel,
    resetTurnPersistState,
    refreshThreadContext,
  });

  const { handleExportTraceReport, handleExportTraceCompare } = useTraceExport(resumedThreadId, t);

  useEffect(() => {
    messagesRef.current = messages;
  }, [messages]);

  const toggleDevtools = useCallback(() => {
    if (!desktopHost) return;
    void import('@tauri-apps/api/core').then(({ invoke }) =>
      invoke('plugin:webview|internal_toggle_devtools'),
    );
  }, [desktopHost]);

  const toggleFocusMode = useCallback(() => {
    setFocusMode((prev) => {
      const next = !prev;
      toast.info(next ? t('focusMode.enter') : t('focusMode.exit'), { duration: 2000 });
      return next;
    });
  }, [t]);

  useKeyboardShortcuts([
    { key: 'k', ctrl: true, description: t('keyboard.newSession'), handler: () => handleNewSessionPreserveMode() },
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
    {
      key: '.',
      ctrl: true,
      global: true,
      description: t('keyboard.focusMode'),
      handler: () => toggleFocusMode(),
    },
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

  useEffect(() => {
    suppressChecklistAutoSwitchRef.current = false;
    suppressAuditAutoSwitchRef.current = false;
    setAuditGridDismissed(false);
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
      setAuditGridDismissed(true);
      acknowledgeInspectorView(view);
      setActiveInspector(view);
      setRightPanelCollapsed(false);
    },
    [activeInspector, acknowledgeInspectorView],
  );

  const handleRequestChecklist = useCallback(() => {
    /* Harness float stack auto-shows via useHarnessGridData when checklist data exists. */
  }, []);

  const handleRequestAudit = useCallback(() => {
    /* Harness float stack auto-shows via useHarnessGridData when scratchpad data exists. */
  }, []);

  useEffect(() => {
    writeSessionStripOpen(sessionStripOpen);
  }, [sessionStripOpen]);

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  useEffect(() => {
    try {
      localStorage.setItem('zagens-desktop-model', selectedModel);
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
      localStorage.setItem('zagens-desktop-run-mode', runMode);
    } catch {
      /* ignore */
    }
  }, [runMode]);

  const taskTypePersistReadyRef = useRef(false);
  useEffect(() => {
    if (!taskTypePersistReadyRef.current) {
      taskTypePersistReadyRef.current = true;
      return;
    }
    void syncTaskTypePreferencePersist(taskTypePreference);
  }, [taskTypePreference]);

  useEffect(() => {
    lockedThreadTaskTypeRef.current = lockedThreadTaskType;
  }, [lockedThreadTaskType]);

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

  const handleThemeChange = useCallback((next: Theme) => {
    setTheme(next);
    try {
      localStorage.setItem('deepseek-theme', next);
    } catch {
      /* ignore */
    }
  }, []);

  const toggleTheme = useCallback(() => {
    handleThemeChange(theme === 'light' ? 'dark' : 'light');
  }, [theme, handleThemeChange]);

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
    if (!officeSession) return;
    if (
      activeInspector === 'agents' ||
      activeInspector === 'index' ||
      activeInspector === 'checklist' ||
      activeInspector === 'audit' ||
      activeInspector === 'long-horizon' ||
      activeInspector === 'routing' ||
      activeInspector === 'lht-settings'
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
          handleNewSessionPreserveMode();
        }
        setTaskTypePreference(next);
        if (next === 'office') {
          setRunMode('agent');
          setAutoApprove(true);
          await applyOfficeDefaultWorkspace(setSelectedWorkspace);
        }
      })();
    },
    [resumedThreadId, handleNewSessionPreserveMode],
  );

  const handleOpenRouting = useCallback(() => {
    setRightPanelCollapsed(false);
    setActiveInspector('routing');
  }, []);

  const handleOpenTasks = useCallback((taskId?: string) => {
    if (taskId) {
      setHighlightTaskId(taskId);
    }
    setActiveInspector('tasks');
    setRightPanelCollapsed(false);
  }, []);

  const handleOpenTaskThread = useCallback(
    (threadId: string) => {
      void (async () => {
        await handleOpenThreadById(threadId);
        setRightPanelCollapsed(true);
      })();
    },
    [handleOpenThreadById],
  );

  useEffect(() => {
    if (!highlightTaskId) return;
    const timer = window.setTimeout(() => setHighlightTaskId(null), 12_000);
    return () => window.clearTimeout(timer);
  }, [highlightTaskId]);

  const handleEnableTrust = useCallback(async () => {
    if (!resumedThreadId) return;
    try {
      await patchThread(resumedThreadId, { trust_mode: true });
      setThreadTrustMode(true);
      toast.dismissAll();
    } catch (e) {
      const err = e as Error & { status?: number };
      toast.error(t('banner.trustModeFailed', { message: err.message }));
    }
  }, [resumedThreadId, t]);

  const auditActivity = useAuditNavActivity({
    threadId: resumedThreadId,
    activeInspector,
    streaming,
    runtimeSessionEstablished,
    narrativeSpawnSuspected,
  });

  const auditGridData = useHarnessGridData({
    threadId: resumedThreadId,
    streaming,
    runtimeSessionEstablished,
    agentStates,
  });
  const auditGridAvailable = !officeSession && auditGridData.hasAnyData;
  const auditGridVisible = auditGridAvailable && !auditGridDismissed;

  useEffect(() => {
    if (!auditGridData.hasAnyData) {
      setAuditGridDismissed(false);
    }
  }, [auditGridData.hasAnyData]);

  const handleToggleAuditGrid = useCallback(() => {
    setAuditGridDismissed((dismissed) => !dismissed);
  }, []);

  const showConnectSplash =
    desktopHost && !bootMainEntered && runtimeConn !== 'connected';
  const showOnboardingWizard =
    desktopHost && shellPrefsReady && startupOverlayOpen && bootMainEntered;

  useEffect(() => {
    if (desktopHost && runtimeConn === 'connected') {
      setBootMainEntered(true);
    }
  }, [desktopHost, runtimeConn]);

  useEffect(() => {
    if (showConnectSplash) {
      dismissRuntimeTransient();
    }
  }, [showConnectSplash, dismissRuntimeTransient, runtimeConn]);

  if (shellInitFailed) {
    return <ShellLoadFailure onRetry={refreshApiKeyStatus} />;
  }

  return (
    <>
      {showConnectSplash ? (
        <StartupConnectOverlay runtimeConn={runtimeConn} onRetry={retryConnect} />
      ) : null}
      {showOnboardingWizard ? (
        <OnboardingOverlay
          apiKeyConfigured={desktopApiKeyConfigured}
          needsKeyStep={desktopApiKeyConfigured === false}
          needsModeStep={!onboardingComplete}
          refreshApiKeyStatus={refreshApiKeyStatus}
          taskTypePreference={taskTypePreference}
          onTaskTypePreferenceChange={handleTaskTypePreferenceChange}
          onComplete={(taskType) => {
            if (taskType) markOnboardingComplete(taskType);
            setStartupOverlayOpen(false);
          }}
        />
      ) : null}
      {!showConnectSplash ? (
      <AppShell
      desktopHost={desktopHost}
      storagePauseTurns={storagePauseTurns}
      storageSnapshot={storageSnapshot}
      storageLevel={storageLevel}
      selectedWorkspace={selectedWorkspace}
      approval={approval}
      approvalBusy={approvalBusy}
      onApproveDecision={(decision, rememberForSession) =>
        void handleApproveDecision(decision, rememberForSession)
      }
      modelParamsOpen={modelParamsOpen}
      modelParams={modelParams}
      onModelParamsOpenChange={setModelParamsOpen}
      onModelParamsApply={(params) => {
        setModelParams(params);
        saveModelParams(params);
      }}
      editDraft={editDraft}
      onEditDraftChange={setEditDraft}
      onConfirmEdit={handleConfirmEdit}
      backtrackDraft={backtrackDraft}
      backtrackBusy={backtrackBusy}
      onBacktrackDraftChange={setBacktrackDraft}
      onConfirmBacktrack={() => void handleConfirmBacktrack()}
      visibleSessions={visibleSessions}
      showAllSessions={showAllSessions}
      onToggleShowAllSessions={() => setShowAllSessions((v) => !v)}
      activeSessionId={activeSessionId}
      streamingSessionIds={streamingSessionIds}
      onNewSession={handleNewSessionPreserveMode}
      onSelectSession={handleSelectSession}
      onDeleteSession={handleDeleteSession}
      runtimeConn={runtimeConn}
      streaming={streaming}
      runtimeSessionEstablished={runtimeSessionEstablished}
      desktopApiKeyConfigured={desktopApiKeyConfigured}
      activeInspector={activeInspector}
      onInspectorChange={handleInspectorChange}
      sessionStripOpen={sessionStripOpen}
      onToggleSessionStrip={() => setSessionStripOpen((open) => !open)}
      harnessGridData={auditGridData}
      userDismissedHarness={auditGridDismissed}
      onShowHarnessStack={() => setAuditGridDismissed(false)}
      focusMode={focusMode}
      officeSession={officeSession}
      checklistActivity={checklistActivity}
      auditActivity={auditActivity}
      taskActivity={taskActivity}
      agentActivity={agentActivity}
      onSend={handleSend}
      onCancelStream={handleCancelStream}
      autoApprove={autoApprove}
      approvalPolicy={approvalPolicy}
      onAutoApproveChange={handleAutoApproveChange}
      runMode={runMode}
      onRunModeChange={handleRunModeChange}
      taskTypePreference={taskTypePreference}
      lockedThreadTaskType={lockedThreadTaskType}
      onTaskTypePreferenceChange={handleTaskTypePreferenceChange}
      routeIntent={routeIntent}
      onOpenRouting={handleOpenRouting}
      onExportSessionJson={() => void handleExportSessionJson()}
      onExportThreadJson={() => void handleExportThreadJson()}
      onExportTraceReport={() => void handleExportTraceReport()}
      onExportTraceCompare={() => void handleExportTraceCompare()}
      selectedModel={selectedModel}
      onModelChange={setSelectedModel}
      composerModelOptions={composerModelOptions}
      onComposerWorkspaceChange={handleComposerWorkspaceChange}
      useWorktree={useWorktree}
      onUseWorktreeChange={(next) => {
        setUseWorktree(next);
        persistUseWorktreePreference(next);
      }}
      activeWorktreeName={activeWorktreeName}
      resumedThreadId={resumedThreadId}
      contextUsagePct={contextUsagePct}
      contextUsedTokens={contextUsedTokens}
      contextWindowTokens={contextWindowTokens}
      threadContextSnapshot={threadContextSnapshot}
      lastTurnOutputTokens={lastTurnOutputTokens}
      lastCacheHitPercent={lastCacheHitPercent}
      lhtChip={lhtChip}
      composerMention={
        composerMentionRel
          ? {
              relPath: composerMentionRel,
              isDirectory: composerMentionIsDir,
              nonce: composerMentionNonce,
            }
          : undefined
      }
      composerPrefill={composerPrefill}
      onOfficeQuickStart={(text) =>
        setComposerPrefill({ text: text.trim(), nonce: Date.now() })
      }
      messages={messages}
      sessionRestoreLoading={sessionRestoreLoading}
      sessionRestoreSource={sessionRestoreSource}
      onRetrySessionRestore={retrySessionRestore}
      agentStates={agentStates}
      onChatOpenWorkspacePath={(rel) => void handleChatOpenWorkspacePath(rel)}
      onRevealWorkspacePath={revealWorkspaceFileInDirectory}
      onOpenDiffInPanel={openDiffInPanel}
      onEditMessage={resumedThreadId ? handleEditMessage : undefined}
      onBacktrackFromMessage={resumedThreadId ? handleBacktrackFromMessage : undefined}
      rightPanelCollapsed={rightPanelCollapsed}
      onExpandRightPanel={() => setRightPanelCollapsed(false)}
      onCollapseRightPanel={() => setRightPanelCollapsed(true)}
      theme={theme}
      onToggleTheme={toggleTheme}
      onThemeChange={handleThemeChange}
      platform={platform}
      threadTrustMode={threadTrustMode}
      onEnableTrust={handleEnableTrust}
      panelPreview={panelPreview}
      onClosePreview={closePanelPreview}
      openWorkspaceFile={openWorkspaceFileForPreview}
      revealWorkspaceFile={revealWorkspaceFileInDirectory}
      addWorkspaceFileToChat={addWorkspaceFileToChat}
      focusFilesNonce={focusWorkspaceFilesNonce}
      focusFilesRelPath={focusWorkspaceFilesRelPath}
      filesRefreshNonce={filesRefreshNonce}
      focusDiffNonce={focusWorkspaceDiffNonce}
      onRequestChecklist={handleRequestChecklist}
      onRequestAudit={handleRequestAudit}
      auditGridVisible={auditGridVisible}
      auditGridAvailable={auditGridAvailable}
      onToggleAuditGrid={handleToggleAuditGrid}
      subagentActiveCount={subagentActiveCount}
      narrativeSpawnSuspected={narrativeSpawnSuspected}
      onRequestMermaid={() => setActiveInspector('mermaid')}
      onRequestDiff={handleRequestDiffPanel}
      onSystemSettingsSaved={handleSystemSettingsSavedWithModels}
      onRouteIntentChange={setRouteIntent}
      refreshApiKeyStatus={refreshApiKeyStatus}
      onModelProvidersSaved={handleModelProvidersSaved}
      onOpenTasks={handleOpenTasks}
      onOpenTaskThread={handleOpenTaskThread}
      highlightTaskId={highlightTaskId}
      />
      ) : null}
    </>
  );
}
