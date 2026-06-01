import { useCallback, useEffect, useRef, useState, type Dispatch, type SetStateAction } from 'react';
import { useInspectorUnread } from './lib/useInspectorUnread';
import {
  getThreadDetail,
  patchThread,
} from './api/client';
import { useT } from './i18n';
import AppShell from './components/AppShell';
import OnboardingOverlay from './components/OnboardingOverlay';
import { useAuditNavActivity } from './lib/useAuditNavActivity';
import { useHarnessGridData } from './lib/useHarnessGridData';
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
import { useDesktopShell } from './hooks/useDesktopShell';
import { useSessionNavigation } from './hooks/useSessionNavigation';
import { useThreadContext } from './hooks/useThreadContext';
import { useWorkspacePanel } from './hooks/useWorkspacePanel';
import { useTurnSession } from './hooks/useTurnSession';
import { useTurnApproval, type ApprovalState } from './hooks/useTurnApproval';
import { useTurnStream } from './hooks/useTurnStream';
import { useTurnSend, type TurnChatMessage } from './hooks/useTurnSend';
import {
  type DesktopModelId,
  type DesktopRouteIntentOption,
  type DesktopRunModeId,
  type DesktopTaskTypePreference,
  type DesktopTaskTypeResolved,
  parseDesktopTaskTypeResolved,
} from './types/desktop';
import {
  applyOfficeDefaultWorkspace,
} from './lib/defaultWorkspace';
import {
  ACTIVE_INSPECTOR_STORAGE_KEY,
  applyTheme,
  ensureDefaultComposerWorkspace,
  isOnboarded,
  markOnboarded,
  loadComposerPrefs,
  loadRouteIntentPreference,
  loadRunModePreference,
  loadStoredInspector,
  loadStoredRightPanelCollapsed,
  loadTaskTypePreference,
  loadTheme,
  RIGHT_PANEL_COLLAPSED_STORAGE_KEY,
  ROUTE_INTENT_STORAGE_KEY,
  TASK_TYPE_STORAGE_KEY,
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
  const [selectedModel, setSelectedModel] = useState<DesktopModelId>(() => loadComposerPrefs(getWindowLabel()).model);
  const [selectedWorkspace, setSelectedWorkspace] = useState(() => loadComposerPrefs(getWindowLabel()).workspace);
  const [messages, setMessages] = useState<TurnChatMessage[]>([]);
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
  const [onboardingState, setOnboardingState] = useState<'unknown' | 'show' | 'hidden'>(() =>
    isOnboarded() ? 'hidden' : 'unknown',
  );

  const refreshSessionsRef = useRef<() => Promise<void>>(async () => {});
  const setRuntimeSessionEstablishedRef = useRef<Dispatch<SetStateAction<boolean>>>(() => {});
  const notifyRuntimeTransientRef = useRef<(message: string) => void>(() => {});
  const reconcileRuntimeAfterFetchFailureRef = useRef<() => void>(() => {});
  const handleSelectSessionRef = useRef<(sessionId: string) => void>(() => {});
  const handleNewSessionRef = useRef<() => void>(() => {});
  const streamingRef = useRef(false);
  const setApprovalRef = useRef<(value: ApprovalState | null) => void>(() => {});
  const setLastTurnOutputTokensRef = useRef<(value: number | null) => void>(() => {});
  const messagesRef = useRef<TurnChatMessage[]>([]);
  const sessionUiCacheRef = useRef<Map<string, CachedUiMessage[]>>(new Map());
  const suppressChecklistAutoSwitchRef = useRef(false);
  const suppressAuditAutoSwitchRef = useRef(false);

  const [lastTurnOutputTokens, setLastTurnOutputTokens] = useState<number | null>(null);
  const [lastCacheHitPercent, setLastCacheHitPercent] = useState<number | null>(null);
  const [lhtChip, setLhtChip] = useState<LhtChipState | null>(null);

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
      setLastCacheHitPercent(null);
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

  const { desktopHost, desktopApiKeyConfigured, platform, refreshApiKeyStatus } = useDesktopShell({
    t,
    selectedWorkspace,
    setSelectedWorkspace,
    streamingRef,
    streamControllersRef,
    streamSessionRef,
    setStreamingThreadIds,
    setPendingComposerStream,
    setMessages,
    notifyRuntimeTransient,
  });

  // First-run decision (runs once): show guided setup only for fresh desktop
  // installs without a key; silently mark existing/keyed users as onboarded.
  useEffect(() => {
    if (onboardingState !== 'unknown') return;
    if (!desktopHost) return;
    if (desktopApiKeyConfigured === null) return;
    if (desktopApiKeyConfigured === true) {
      markOnboarded();
      setOnboardingState('hidden');
    } else {
      setOnboardingState('show');
    }
  }, [onboardingState, desktopHost, desktopApiKeyConfigured]);

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
  } = useAgentPanelState({
    messages,
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
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [rightPanelCollapsed, setRightPanelCollapsed] = useState(() =>
    loadStoredRightPanelCollapsed(),
  );
  const [auditGridDismissed, setAuditGridDismissed] = useState(false);

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
  });

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
  });

  const { handleSelectSession, handleNewSession } = useSessionNavigation({
    t,
    selectedModel,
    activeSessionIdRef,
    resumedThreadIdRef,
    threadTurnRef,
    threadContextSnapshotRef,
    threadContextCacheRef,
    messagesRef,
    sessionUiCacheRef,
    abortThreadStream,
    resetTurnPersistState,
    clearApproval,
    setMessages,
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
  });

  handleSelectSessionRef.current = handleSelectSession;
  handleNewSessionRef.current = handleNewSession;

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
    threadTurnRef,
    streamControllersRef,
    sessionUiCacheRef,
    handleSend,
    setMessages,
    setResumedThreadId,
    setStreamingThreadIds,
    setPendingComposerStream,
    setThreadDetailForContext,
    setLastTurnOutputTokens,
    setLastCacheHitPercent,
    setComposerPrefill,
    resetAgentPanel,
    resetTurnPersistState,
    refreshThreadContext,
  });

  useEffect(() => {
    messagesRef.current = messages;
  }, [messages]);

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
    /* Audit grid auto-shows via useAuditGridData when checklist data exists. */
  }, []);

  const handleRequestAudit = useCallback(() => {
    /* Audit grid auto-shows via useAuditGridData when scratchpad data exists. */
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

  const handleOpenRouting = useCallback(() => {
    setRightPanelCollapsed(false);
    setActiveInspector('routing');
  }, []);

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

  const handleDismissAuditGrid = useCallback(() => {
    setAuditGridDismissed(true);
  }, []);

  return (
    <>
      {onboardingState === 'show' && (
        <OnboardingOverlay
          runtimeConn={runtimeConn}
          apiKeyConfigured={desktopApiKeyConfigured}
          refreshApiKeyStatus={refreshApiKeyStatus}
          taskTypePreference={taskTypePreference}
          onTaskTypePreferenceChange={handleTaskTypePreferenceChange}
          onComplete={() => {
            markOnboarded();
            setOnboardingState('hidden');
          }}
        />
      )}
      <AppShell
      desktopHost={desktopHost}
      selectedWorkspace={selectedWorkspace}
      approval={approval}
      approvalBusy={approvalBusy}
      onApproveDecision={(decision) => void handleApproveDecision(decision)}
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
      onNewSession={handleNewSession}
      onSelectSession={handleSelectSession}
      onDeleteSession={handleDeleteSession}
      runtimeConn={runtimeConn}
      streaming={streaming}
      runtimeSessionEstablished={runtimeSessionEstablished}
      desktopApiKeyConfigured={desktopApiKeyConfigured}
      activeInspector={activeInspector}
      onInspectorChange={handleInspectorChange}
      sidebarCollapsed={sidebarCollapsed}
      onToggleSidebarCollapse={() => setSidebarCollapsed((v) => !v)}
      onExpandSidebar={() => setSidebarCollapsed(false)}
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
      selectedModel={selectedModel}
      onModelChange={setSelectedModel}
      onComposerWorkspaceChange={handleComposerWorkspaceChange}
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
      messages={messages}
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
      focusDiffNonce={focusWorkspaceDiffNonce}
      onRequestChecklist={handleRequestChecklist}
      onRequestAudit={handleRequestAudit}
      auditGridVisible={auditGridVisible}
      auditGridAvailable={auditGridAvailable}
      onToggleAuditGrid={handleToggleAuditGrid}
      onDismissAuditGrid={handleDismissAuditGrid}
      subagentActiveCount={subagentActiveCount}
      narrativeSpawnSuspected={narrativeSpawnSuspected}
      onRequestMermaid={() => setActiveInspector('mermaid')}
      onRequestDiff={handleRequestDiffPanel}
      onSystemSettingsSaved={handleSystemSettingsSaved}
      onRouteIntentChange={setRouteIntent}
      refreshApiKeyStatus={refreshApiKeyStatus}
      />
    </>
  );
}
