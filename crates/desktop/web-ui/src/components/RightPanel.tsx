import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { extractDiffRelPaths } from '../lib/diff/diffEntries';
import type { invoke as InvokeFn } from '@tauri-apps/api/core';
import ModelProvidersPanel from './ModelProvidersPanel';
import McpPanel from './McpPanel';
import UsageDashboard from './UsageDashboard';
import AgentHealthPanel from './AgentHealthPanel';
import NightQueuePanel from './NightQueuePanel';
import AutomationPanel from './AutomationPanel';
import AgentPanel from './AgentPanel';
import TopicMemoryPanel from './TopicMemoryPanel';
import RoutingPanel from './RoutingPanel';
import ChecklistPanel from './ChecklistPanel';
import MermaidPanel from './MermaidPanel';
import SettingsPanel from './SettingsPanel';
import SandboxSettingsPanel from './SandboxSettingsPanel';
import LhtSettingsPanel from './LhtSettingsPanel';
import HooksPanel from './HooksPanel';
import ScheduledAutomationsPanel from './ScheduledAutomationsPanel';
import IndexPanel from './IndexPanel';
import TerminalPanel from './terminal/TerminalPanel';
import DiffPanel from './diff/DiffPanel';
import type { AgentState } from '../types/agent';
import {
  PreviewContainer,
  PreviewDispatcher,
} from './preview';
import type { PreviewState } from './preview/types';
import type { RuntimeConnectionState } from '../api/client';
import type { DesktopRouteIntentOption } from '../types/desktop';
import { useT } from '../i18n';
import type { TranslationKey } from '../i18n/keys';
import { getThreadSnapshots, restoreThreadSnapshot } from '../api/client';
import WorkspaceFilesPanel from './WorkspaceFilesPanel';
import { confirmDialog } from '../lib/confirmDialog';
import { isRuntimeApiAvailable } from '../lib/runtimeReachable';
import { toast } from '../lib/toast';
import PanelEdgeSeam from './PanelEdgeSeam';
import AboutPanel from './AboutPanel';
import AuditScratchpadPanel from './AuditScratchpadPanel';
import LongHorizonPanel from './LongHorizonPanel';
import InspectorIconTabs from './chrome/InspectorIconTabs';
import BrowserPane from './browser/BrowserPane';

export type RightPanelView =
  | 'workspace'
  | 'models'
  | 'settings'
  | 'system'
  | 'sandbox'
  | 'lht-settings'
  | 'hooks'
  | 'schedule'
  | 'mcp'
  | 'usage'
  | 'agent-health'
  | 'night-queue'
  | 'tasks'
  | 'skills'
  | 'agents'
  | 'topic-memory'
  | 'routing'
  | 'index'
  | 'checklist'
  | 'audit'
  | 'long-horizon'
  | 'mermaid'
  | 'about'
  | 'browser';

export type WorkspaceTabId = 'restore' | 'files' | 'rules' | 'terminal' | 'diff';

type Theme = import('../lib/appPreferences').Theme;

const WORKSPACE_TAB_KEY = 'zagens-desktop-right-workspace-tab';
const PANEL_WIDTH_KEY = 'zagens-desktop-right-panel-width';
const PANEL_MIN_PX = 260;
const PANEL_DEFAULT_PX = 320;

function clampPanelWidth(px: number): number {
  if (typeof window === 'undefined') {
    return Math.max(PANEL_MIN_PX, Math.round(px));
  }
  const cap = Math.min(1400, Math.floor(window.innerWidth * 0.8));
  return Math.min(cap, Math.max(PANEL_MIN_PX, Math.round(px)));
}

function readStoredPanelWidth(): number {
  try {
    const n = parseInt(localStorage.getItem(PANEL_WIDTH_KEY) ?? '', 10);
    if (Number.isFinite(n)) {
      return clampPanelWidth(n);
    }
  } catch {
    /* ignore */
  }
  return PANEL_DEFAULT_PX;
}

interface Props {
  view: RightPanelView;
  desktopHost: boolean;
  runtimeConn: RuntimeConnectionState;
  runtimeSessionEstablished?: boolean;
  apiKeyConfigured: boolean | null;
  onSavedApiKey: () => void;
  theme: Theme;
  onToggleTheme: () => void;
  platform: string;
  /** Current composer / thread workspace directory */
  workspaceRoot: string;
  /** Active runtime thread when session resumed — used for restore copy */
  resumedThreadId: string | null;
  /** From runtime thread detail — restore requires trust on server */
  threadTrustMode: boolean;
  onEnableTrust: () => Promise<void>;
  /** Preview overlay state (owned by App — chat bubbles can open files here too). */
  preview: PreviewState | null;
  onClosePreview: () => void;
  openWorkspaceFile: (relPath: string, title?: string) => Promise<void>;
  /** Reveal path in Files tab without opening preview. */
  revealWorkspaceFile?: (relPath: string) => void;
  addWorkspaceFileToChat?: (relPath: string, isDirectory?: boolean) => void;
  /** Bumped when parent wants the workspace panel to show the Files tab (e.g. chat link). */
  focusFilesNonce: number;
  /** Optional path to reveal when `focusFilesNonce` bumps (parent dir opened). */
  focusFilesRelPath?: string | null;
  /** Bumped when chat or auto-detect should show the Diff workspace tab. */
  focusDiffNonce: number;
  focusWorkspaceTab?: WorkspaceTabId | null;
  focusWorkspaceTabNonce?: number;
  onNavigateContextCategory?: (categoryId: string) => void;
  onArchiveContext?: () => void;
  archivePending?: boolean;
  canArchiveContext?: boolean;
  onRequestDiff?: () => void;
  agentStates: AgentState[];
  /** Called when ChecklistPanel detects first data — parent switches view. */
  onRequestChecklist?: () => void;
  /** Called when AuditScratchpadPanel detects first run — parent may switch view. */
  onRequestAudit?: () => void;
  subagentActiveCount?: number;
  narrativeSpawnSuspected?: boolean;
  /** Model turn in progress — checklist panel polls faster. */
  streaming?: boolean;
  /** Chat messages — used by MermaidPanel to extract mermaid code blocks. */
  messages: { id: string; role: string; content: string }[];
  /** Called when MermaidPanel detects first mermaid block — parent switches view. */
  onRequestMermaid?: () => void;
  /** Called when user clicks collapse button in panel header. */
  onCollapse?: () => void;
  routeIntent: DesktopRouteIntentOption;
  onRouteIntentChange: (v: DesktopRouteIntentOption) => void;
  officeSession?: boolean;
  onSystemSettingsSaved?: (settings: import('../api/client').SystemSettings) => void;
  /** Parent bump refreshes workspace file list (e.g. after write_office). */
  filesRefreshNonce?: number;
  /** Open tasks panel; optional task id to highlight. */
  onOpenTasks?: (taskId?: string) => void;
  onOpenTaskThread?: (threadId: string) => void;
  /** Highlight a task row in the tasks panel. */
  highlightTaskId?: string | null;
}

const PANEL_TITLE_KEYS: Record<RightPanelView, TranslationKey> = {
  workspace: 'panels.workspace',
  'models': 'panels.models',
  settings: 'panels.settings',
  system: 'panels.system',
  sandbox: 'panels.sandbox',
  'lht-settings': 'panels.lhtSettings',
  hooks: 'panels.hooks',
  schedule: 'panels.schedule',
  mcp: 'panels.mcp',
  usage: 'panels.usage',
  'agent-health': 'panels.agentHealth',
  'night-queue': 'panels.nightQueue',
  tasks: 'panels.tasks',
  skills: 'panels.skills',
  agents: 'panels.agents',
  'topic-memory': 'panels.topicMemory',
  routing: 'panels.routing',
  index: 'panels.index',
  checklist: 'panels.checklist',
  audit: 'panels.audit',
  'long-horizon': 'panels.longHorizon',
  mermaid: 'panels.mermaid',
  about: 'panels.about',
  browser: 'panels.browser',
};

function formatSnapshotTime(ts: number): string {
  try {
    return new Date(ts * 1000).toLocaleString();
  } catch {
    return String(ts);
  }
}

export default function RightPanel({
  view,
  desktopHost,
  runtimeConn,
  runtimeSessionEstablished = false,
  apiKeyConfigured,
  onSavedApiKey,
  theme,
  onToggleTheme,
  platform,
  workspaceRoot,
  resumedThreadId,
  threadTrustMode,
  onEnableTrust,
  preview,
  onClosePreview,
  openWorkspaceFile,
  revealWorkspaceFile,
  addWorkspaceFileToChat,
  focusFilesNonce,
  focusFilesRelPath,
  focusDiffNonce,
  focusWorkspaceTab = null,
  focusWorkspaceTabNonce = 0,
  onNavigateContextCategory,
  onArchiveContext,
  archivePending = false,
  canArchiveContext = false,
  onRequestDiff,
  agentStates,
  onRequestChecklist,
  onRequestAudit,
  streaming = false,
  messages,
  onRequestMermaid,
  onCollapse,
  routeIntent,
  onRouteIntentChange,
  officeSession = false,
  onSystemSettingsSaved,
  filesRefreshNonce: filesRefreshNonceProp = 0,
  onOpenTasks,
  onOpenTaskThread,
  highlightTaskId = null,
  subagentActiveCount = 0,
  narrativeSpawnSuspected = false,
}: Props) {
  const { t } = useT();
  const officeChangePaths = useMemo(
    () => (officeSession ? extractDiffRelPaths(messages) : []),
    [officeSession, messages],
  );
  const [workspaceTab, setWorkspaceTab] = useState<WorkspaceTabId>(() => {
    try {
      const s = sessionStorage.getItem(WORKSPACE_TAB_KEY);
      if (s === 'restore' || s === 'files' || s === 'rules' || s === 'terminal' || s === 'diff')
        return s;
    } catch {
      /* ignore */
    }
    return 'files';
  });

  const [snapshots, setSnapshots] = useState<
    Array<{ n: number; id: string; label: string; timestamp: number }>
  >([]);
  const [snapLoading, setSnapLoading] = useState(false);
  const [snapError, setSnapError] = useState<string | null>(null);
  const [restoreBusy, setRestoreBusy] = useState<number | null>(null);
  const [restoreMessage, setRestoreMessage] = useState<string | null>(null);

  const [rebuildingIndex, setRebuildingIndex] = useState(false);
  const [rebuildIndexError, setRebuildIndexError] = useState<string | null>(null);

  const [pickRulesBody, setPickRulesBody] = useState('');
  const [pickRulesLoading, setPickRulesLoading] = useState(false);
  const [pickRulesSaving, setPickRulesSaving] = useState(false);
  const [pickRulesErr, setPickRulesErr] = useState<string | null>(null);
  const [pickRulesOk, setPickRulesOk] = useState<string | null>(null);
  const [filesRefreshNonceLocal, setFilesRefreshNonceLocal] = useState(0);
  const filesRefreshNonce = filesRefreshNonceProp + filesRefreshNonceLocal;

  const runtimeReach = {
    streaming: Boolean(streaming),
    sessionEstablished: runtimeSessionEstablished,
  };
  const runtimeOk = isRuntimeApiAvailable(runtimeConn, runtimeReach);


  useEffect(() => {
    try {
      sessionStorage.setItem(WORKSPACE_TAB_KEY, workspaceTab);
    } catch {
      /* ignore */
    }
  }, [workspaceTab]);

  useEffect(() => {
    setSnapshots([]);
    setSnapError(null);
    setRestoreMessage(null);
  }, [resumedThreadId, workspaceRoot]);

  useEffect(() => {
    if (!officeSession) return;
    if (
      workspaceTab === 'restore' ||
      workspaceTab === 'rules' ||
      workspaceTab === 'terminal' ||
      workspaceTab === 'diff'
    ) {
      setWorkspaceTab('files');
    }
  }, [officeSession, workspaceTab]);

  useEffect(() => {
    if (view !== 'workspace' || workspaceTab !== 'rules' || !desktopHost) {
      return;
    }
    const root = workspaceRoot.trim();
    if (!root) {
      setPickRulesBody('');
      setPickRulesLoading(false);
      setPickRulesErr(null);
      setPickRulesOk(null);
      return;
    }
    let cancelled = false;
    setPickRulesLoading(true);
    setPickRulesErr(null);
    setPickRulesOk(null);
    void (async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const text = await invoke<string>('read_pick_rules', { workspaceRoot: root });
        if (!cancelled) {
          setPickRulesBody(text);
        }
      } catch (e) {
        if (!cancelled) {
          const msg = e instanceof Error ? e.message : String(e);
          setPickRulesErr(t('workspaceRules.loadError', { message: msg }));
        }
      } finally {
        if (!cancelled) {
          setPickRulesLoading(false);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [view, workspaceTab, desktopHost, workspaceRoot, t]);

  useEffect(() => {
    if (view !== 'workspace' || workspaceTab !== 'restore' || !resumedThreadId || !runtimeOk) {
      return;
    }
    let cancelled = false;
    setSnapLoading(true);
    setSnapError(null);
    void getThreadSnapshots(resumedThreadId, { limit: 50 })
      .then((res) => {
        if (cancelled) return;
        setSnapshots(res.snapshots ?? []);
      })
      .catch((e) => {
        if (cancelled) return;
        const err = e as Error & { status?: number };
        setSnapError(err.message ?? String(e));
        setSnapshots([]);
      })
      .finally(() => {
        if (!cancelled) setSnapLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [view, workspaceTab, resumedThreadId, runtimeOk]);

  useEffect(() => {
    if (focusFilesNonce > 0) {
      setWorkspaceTab('files');
    }
  }, [focusFilesNonce]);

  useEffect(() => {
    if (focusDiffNonce > 0) {
      setWorkspaceTab('diff');
    }
  }, [focusDiffNonce]);

  useEffect(() => {
    if (focusWorkspaceTabNonce > 0 && focusWorkspaceTab) {
      setWorkspaceTab(focusWorkspaceTab);
    }
  }, [focusWorkspaceTab, focusWorkspaceTabNonce]);

  const onRestore = useCallback(
    async (n: number) => {
      if (!resumedThreadId || !runtimeOk) return;
      if (!(await confirmDialog(t('workbench.restoreConfirm', { n: String(n) })))) return;
      setRestoreBusy(n);
      setRestoreMessage(null);
      try {
        const r = await restoreThreadSnapshot(resumedThreadId, n);
        setRestoreMessage(
          t('workbench.restoreSuccess', {
            label: r.label,
            id: r.id.slice(0, 8),
          }),
        );
        const list = await getThreadSnapshots(resumedThreadId, { limit: 50 });
        setSnapshots(list.snapshots ?? []);
        setFilesRefreshNonceLocal((x) => x + 1);
      } catch (e) {
        const err = e as Error & { status?: number };
        if (err.status === 403) {
          setRestoreMessage(t('workbench.restoreTrustRequired'));
        } else {
          setRestoreMessage(
            t('workbench.restoreFailed', { message: err.message ?? String(e) }),
          );
        }
      } finally {
        setRestoreBusy(null);
      }
    },
    [resumedThreadId, runtimeOk, t],
  );

  const onRebuildIndex = useCallback(async () => {
    if (!desktopHost) return;
    setRebuildingIndex(true);
    setRebuildIndexError(null);
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('rebuild_symbol_index', { workspace: workspaceRoot });
      toast.success(t('indexPanel.rebuildSuccess'));
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setRebuildIndexError(msg);
    } finally {
      setRebuildingIndex(false);
    }
  }, [desktopHost, workspaceRoot, t]);

  const onEnableTrustClick = useCallback(async () => {
    try {
      await onEnableTrust();
      setRestoreMessage(null);
    } catch {
      /* parent sets banner */
    }
  }, [onEnableTrust]);

  const savePickRules = useCallback(async () => {
    if (!desktopHost) {
      return;
    }
    const root = workspaceRoot.trim();
    if (!root) {
      return;
    }
    setPickRulesSaving(true);
    setPickRulesErr(null);
    setPickRulesOk(null);
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('save_pick_rules', { workspaceRoot: root, content: pickRulesBody });
      toast.success(t('workspaceRules.saved'));
      setPickRulesOk(null);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setPickRulesErr(t('workspaceRules.saveError', { message: msg }));
    } finally {
      setPickRulesSaving(false);
    }
  }, [desktopHost, workspaceRoot, pickRulesBody, t]);

  const [panelWidth, setPanelWidth] = useState(readStoredPanelWidth);
  const resizeDragRef = useRef<{ pointerId: number; startX: number; startW: number } | null>(null);
  const livePanelWidthRef = useRef(panelWidth);
  const [panelResizing, setPanelResizing] = useState(false);

  useEffect(() => {
    livePanelWidthRef.current = panelWidth;
  }, [panelWidth]);

  useEffect(() => {
    const onResize = () => {
      setPanelWidth((w) => clampPanelWidth(w));
    };
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, []);

  const endPanelResize = useCallback((e: React.PointerEvent) => {
    const el = e.currentTarget as HTMLDivElement;
    const d = resizeDragRef.current;
    if (!d || e.pointerId !== d.pointerId) {
      return;
    }
    resizeDragRef.current = null;
    setPanelResizing(false);
    if (el.hasPointerCapture(e.pointerId)) {
      el.releasePointerCapture(e.pointerId);
    }
    if (e.type === 'pointerup') {
      const next = clampPanelWidth(d.startW - (e.clientX - d.startX));
      setPanelWidth(next);
      try {
        localStorage.setItem(PANEL_WIDTH_KEY, String(next));
      } catch {
        /* ignore */
      }
    } else if (e.type === 'pointercancel') {
      try {
        localStorage.setItem(PANEL_WIDTH_KEY, String(livePanelWidthRef.current));
      } catch {
        /* ignore */
      }
    }
  }, []);

  const onResizePointerDown = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (e.button !== 0) {
        return;
      }
      e.preventDefault();
      resizeDragRef.current = { pointerId: e.pointerId, startX: e.clientX, startW: panelWidth };
      setPanelResizing(true);
      e.currentTarget.setPointerCapture(e.pointerId);
    },
    [panelWidth],
  );

  const visibleWorkspaceTabs = useMemo((): WorkspaceTabId[] => {
    const tabs: WorkspaceTabId[] = [];
    if (!officeSession) {
      tabs.push('restore');
    }
    tabs.push('files');
    if (!officeSession) {
      tabs.push('rules', 'terminal', 'diff');
    }
    return tabs;
  }, [officeSession]);

  const workbenchTabId = (tab: WorkspaceTabId) => `workbench-tab-${tab}`;
  const workbenchTabPanelId = 'workbench-tabpanel';

  const onResizePointerMove = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    const d = resizeDragRef.current;
    if (!d || e.pointerId !== d.pointerId) {
      return;
    }
    const next = clampPanelWidth(d.startW - (e.clientX - d.startX));
    livePanelWidthRef.current = next;
    setPanelWidth(next);
  }, []);

  return (
    <div className="flex h-full max-h-screen shrink-0">
      <PanelEdgeSeam
        side="right"
        seamClass="chrome-seam-l"
        resizing={panelResizing}
        ariaResize={t('rightPanel.resizeWidth')}
        collapseTitle={onCollapse ? t('rightPanel.collapse') : undefined}
        onCollapse={onCollapse}
        onPointerDown={onResizePointerDown}
        onPointerMove={onResizePointerMove}
        onPointerUp={endPanelResize}
        onPointerCancel={endPanelResize}
        onKeyDown={(e) => {
          const step = e.shiftKey ? 32 : 16;
          if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
            e.preventDefault();
            const delta = e.key === 'ArrowLeft' ? step : -step;
            setPanelWidth((w) => {
              const n = clampPanelWidth(w + delta);
              try {
                localStorage.setItem(PANEL_WIDTH_KEY, String(n));
              } catch {
                /* ignore */
              }
              return n;
            });
          }
        }}
      />
      <aside
        role="complementary"
        aria-label={t('workbench.panelAria')}
        className="flex min-w-0 shrink-0 flex-col bg-canvas overflow-hidden"
        style={{ width: panelWidth }}
      >
      <div className="flex shrink-0 items-center bg-canvas-alt/40 px-4 py-3">
        <h2 className="text-sm font-semibold text-t-text">{t(PANEL_TITLE_KEYS[view])}</h2>
      </div>

      <div className="flex-1 flex flex-col min-h-0 overflow-hidden text-sm text-t-text">
        {view === 'workspace' && (
          <>
            {!runtimeOk && (
              <p className="shrink-0 px-3 py-2 text-[11px] text-red-400/90 border-b border-divider bg-red-500/10">
                {t('workbench.runtimeOffline')}
              </p>
            )}
            {runtimeOk && runtimeConn !== 'connected' && (
              <p className="shrink-0 px-3 py-2 text-[11px] text-amber-text/90 border-b border-divider bg-amber-bg/30">
                {t('workbench.runtimeBusy')}
              </p>
            )}
            {preview ? (
              <PreviewContainer title={preview.title} onClose={onClosePreview}>
                <PreviewDispatcher
                  state={preview}
                  theme={theme}
                  onOpenWorkspaceRelativePath={(rel) => {
                    void openWorkspaceFile(rel).catch((err) => {
                      const e = err as Error & { status?: number };
                      toast.error(e.message ?? String(err));
                    });
                  }}
                />
              </PreviewContainer>
            ) : (
              <div className="flex min-h-0 flex-1">
                <InspectorIconTabs
                  tabs={visibleWorkspaceTabs}
                  activeTab={workspaceTab}
                  onTabChange={setWorkspaceTab}
                  tabIdFor={workbenchTabId}
                  tabPanelId={workbenchTabPanelId}
                  ariaLabel={t('workbench.tablistAria')}
                />

                <div
                  id={workbenchTabPanelId}
                  className={`min-h-0 min-w-0 flex-1 ${workspaceTab === 'terminal' || workspaceTab === 'diff' || workspaceTab === 'files' ? 'flex flex-col overflow-hidden' : 'overflow-y-auto'}`}
                  role="tabpanel"
                  aria-labelledby={workbenchTabId(workspaceTab)}
                  tabIndex={0}
                >
                  {workspaceTab === 'restore' && (
                    <div className="p-4 space-y-3 text-xs text-t-text leading-relaxed">
                      <p className="text-t-text-muted">{t('workbench.restoreSyncLine1')}</p>
                      <p className="text-t-text-muted">{t('workbench.restoreSyncLine2')}</p>
                      {!resumedThreadId && (
                        <p className="text-amber-text/90">{t('workbench.restoreNeedThread')}</p>
                      )}
                      {resumedThreadId && (
                        <p className="text-t-text-muted">
                          {t('workbench.currentThread', {
                            id: `${resumedThreadId.slice(0, 14)}…`,
                          })}
                          {!threadTrustMode && (
                            <span className="block mt-1">{t('workbench.restoreNeedsTrust')}</span>
                          )}
                        </p>
                      )}
                      {!threadTrustMode && resumedThreadId && (
                        <button
                          type="button"
                          className="rounded-lg border border-divider px-3 py-2 text-xs font-medium text-accent hover:bg-hover disabled:opacity-50"
                          disabled={!runtimeOk}
                          onClick={() => void onEnableTrustClick()}
                        >
                          {t('workbench.enableTrust')}
                        </button>
                      )}
                      {snapLoading && <p className="text-t-text-muted">{t('workbench.loadingSnapshots')}</p>}
                      {snapError && (
                        <p className="rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2 text-red-200/90">
                          {snapError}
                        </p>
                      )}
                      {restoreMessage && (
                        <p className="rounded-lg border border-divider px-3 py-2 text-t-text-muted">
                          {restoreMessage}
                        </p>
                      )}
                      {resumedThreadId && !snapLoading && snapshots.length === 0 && !snapError && (
                        <p className="text-t-text-muted">{t('workbench.noSnapshots')}</p>
                      )}
                      {snapshots.length > 0 && (
                        <ul className="space-y-2">
                          {snapshots.map((s) => (
                            <li
                              key={`${s.n}-${s.id}`}
                              className="rounded-lg border border-divider px-3 py-2.5 bg-canvas-alt/30"
                            >
                              <div className="flex justify-between gap-2 items-start">
                                <div className="min-w-0 flex-1">
                                  <div className="font-medium text-t-text truncate" title={s.label}>
                                    #{s.n} · {s.label || t('workbench.snapshotUntitled')}
                                  </div>
                                  <div className="text-[10px] text-t-text-muted mt-0.5">
                                    {formatSnapshotTime(s.timestamp)}
                                  </div>
                                  <div className="text-[10px] font-mono text-t-text-muted truncate mt-1">
                                    {s.id}
                                  </div>
                                </div>
                                <button
                                  type="button"
                                  className="shrink-0 rounded-md px-2 py-1 text-[11px] font-medium text-accent border border-divider hover:bg-hover disabled:opacity-40"
                                  disabled={!runtimeOk || restoreBusy !== null || !threadTrustMode}
                                  title={
                                    !threadTrustMode ? t('workbench.restoreTrustTitle') : undefined
                                  }
                                  onClick={() => void onRestore(s.n)}
                                >
                                  {restoreBusy === s.n ? '…' : t('workbench.restoreBtn')}
                                </button>
                              </div>
                            </li>
                          ))}
                        </ul>
                      )}
                    </div>
                  )}

                  {workspaceTab === 'files' && (
                    <WorkspaceFilesPanel
                      active={view === 'workspace'}
                      workspaceRoot={workspaceRoot}
                      resumedThreadId={resumedThreadId}
                      runtimeOk={runtimeOk}
                      desktopHost={desktopHost}
                      officeSession={officeSession}
                      officeChangePaths={officeChangePaths}
                      preview={preview}
                      openWorkspaceFile={openWorkspaceFile}
                      onAddToChat={addWorkspaceFileToChat}
                      focusFilesNonce={focusFilesNonce}
                      focusFilesRelPath={focusFilesRelPath}
                      externalRefreshNonce={filesRefreshNonce}
                    />
                  )}

                  {workspaceTab === 'rules' && (
                    <div className="flex flex-col min-h-0 p-4 gap-3 text-xs text-t-text leading-relaxed">
                      {!desktopHost && (
                        <p className="text-amber-text/90">{t('workspaceRules.desktopOnly')}</p>
                      )}
                      {desktopHost && !workspaceRoot.trim() && (
                        <p className="text-amber-text/90">{t('workspaceRules.needWorkspace')}</p>
                      )}
                      {desktopHost && Boolean(workspaceRoot.trim()) && (
                        <>
                          <div>
                            <p className="text-[11px] font-medium uppercase tracking-wide text-t-text-muted">
                              {t('workspaceRules.pathLabel')}
                            </p>
                            <code className="text-[11px] text-t-text-muted break-all">
                              {t('workspaceRules.pathValue')}
                            </code>
                          </div>
                          <p className="text-t-text-muted">{t('workspaceRules.hint')}</p>
                          <p className="text-[11px] text-t-text-muted">{t('workspaceRules.emptyHint')}</p>
                          {pickRulesLoading && (
                            <p className="text-t-text-muted">{t('automation.loading')}</p>
                          )}
                          {pickRulesErr && (
                            <p className="rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2 text-red-200/90">
                              {pickRulesErr}
                            </p>
                          )}
                          {pickRulesOk && (
                            <p className="rounded-lg border border-divider px-3 py-2 text-t-text-muted">
                              {pickRulesOk}
                            </p>
                          )}
                          <textarea
                            value={pickRulesBody}
                            onChange={(e) => {
                              setPickRulesBody(e.target.value);
                              setPickRulesOk(null);
                            }}
                            disabled={pickRulesLoading || pickRulesSaving}
                            spellCheck={false}
                            className="min-h-[200px] flex-1 w-full rounded-lg border border-divider bg-canvas px-3 py-2 text-xs font-mono text-t-text placeholder:text-t-text-muted focus:outline-none focus:ring-1 focus:ring-accent disabled:opacity-50"
                            placeholder="Markdown / plain text…"
                            aria-label={t('workspaceRules.tab')}
                          />
                          <button
                            type="button"
                            className="shrink-0 self-start rounded-lg border border-divider px-4 py-2 text-xs font-medium text-accent hover:bg-hover disabled:opacity-50"
                            disabled={pickRulesLoading || pickRulesSaving}
                            onClick={() => void savePickRules()}
                          >
                            {pickRulesSaving ? t('workspaceRules.saving') : t('workspaceRules.save')}
                          </button>
                        </>
                      )}
                    </div>
                  )}

                  {workspaceTab === 'terminal' && (
                    <TerminalPanel
                      workspaceRoot={workspaceRoot}
                      desktopHost={desktopHost}
                      active={view === 'workspace' && workspaceTab === 'terminal'}
                    />
                  )}

                  {workspaceTab === 'diff' && (
                    <DiffPanel
                      messages={messages}
                      onRevealInFiles={revealWorkspaceFile}
                      active={view === 'workspace' && workspaceTab === 'diff'}
                      onDetected={
                        view === 'workspace' && workspaceTab === 'diff'
                          ? undefined
                          : () => {
                              setWorkspaceTab('diff');
                              onRequestDiff?.();
                            }
                      }
                    />
                  )}
                </div>
              </div>
            )}
          </>
        )}

        {view === 'models' && (
          <div className="p-4 overflow-y-auto">
            {!desktopHost && (
              <p className="mb-3 text-xs text-amber-text/90 leading-relaxed">
                {t('workbench.apiKeyNotDesktop')}
              </p>
            )}
            {desktopHost && apiKeyConfigured === false && (
              <p className="mb-3 text-xs text-amber-text/90 leading-relaxed">
                {t('workbench.modelsMissingDeepSeek')}
              </p>
            )}
            {desktopHost && (
              <ModelProvidersPanel
                onSaved={onSavedApiKey}
                className={!desktopHost ? 'pointer-events-none opacity-50' : ''}
              />
            )}
          </div>
        )}

        {view === 'mcp' && (
          <McpPanel
            runtimeConn={runtimeConn}
            streaming={streaming}
            runtimeSessionEstablished={runtimeSessionEstablished}
          />
        )}

        {view === 'usage' && (
          <UsageDashboard
            runtimeConn={runtimeConn}
            streaming={streaming}
            runtimeSessionEstablished={runtimeSessionEstablished}
          />
        )}

        {view === 'agent-health' && (
          <AgentHealthPanel
            runtimeConn={runtimeConn}
            streaming={streaming}
            runtimeSessionEstablished={runtimeSessionEstablished}
          />
        )}

        {view === 'night-queue' && (
          <NightQueuePanel
            runtimeConn={runtimeConn}
            streaming={streaming}
            runtimeSessionEstablished={runtimeSessionEstablished}
          />
        )}

        {view === 'tasks' && (
          <AutomationPanel
            variant="tasks"
            runtimeConn={runtimeConn}
            streaming={streaming}
            runtimeSessionEstablished={runtimeSessionEstablished}
            highlightTaskId={highlightTaskId}
            onOpenTaskThread={onOpenTaskThread}
          />
        )}

        {view === 'skills' && (
          <AutomationPanel
            variant="skills"
            runtimeConn={runtimeConn}
            streaming={streaming}
            runtimeSessionEstablished={runtimeSessionEstablished}
          />
        )}

        {view === 'agents' && !officeSession && (
          <AgentPanel
            agents={agentStates}
            workspaceRoot={workspaceRoot}
            runtimeConn={runtimeConn}
            streaming={streaming}
            runtimeSessionEstablished={runtimeSessionEstablished}
          />
        )}

        {view === 'topic-memory' && !officeSession && (
          <TopicMemoryPanel
            runtimeConn={runtimeConn}
            streaming={streaming}
            runtimeSessionEstablished={runtimeSessionEstablished}
          />
        )}

        {view === 'routing' && !officeSession && (
          <RoutingPanel
            runtimeConn={runtimeConn}
            streaming={streaming}
            runtimeSessionEstablished={runtimeSessionEstablished}
            routeIntent={routeIntent}
            onRouteIntentChange={onRouteIntentChange}
          />
        )}

        {/* Always mounted (hidden when inactive) so polling can auto-trigger the view */}
        {!officeSession && (
        <div style={{ display: view === 'checklist' ? undefined : 'none' }}>
          <ChecklistPanel
            threadId={resumedThreadId ?? ''}
            pollFast={streaming || view === 'checklist'}
            onDetected={onRequestChecklist}
          />
        </div>
        )}

        {!officeSession && (
        <div style={{ display: view === 'audit' ? undefined : 'none' }}>
          <AuditScratchpadPanel
            threadId={resumedThreadId ?? ''}
            workspaceRoot={workspaceRoot}
            pollFast={streaming || view === 'audit'}
            onOpenWorkspacePath={openWorkspaceFile}
            subagentActiveCount={subagentActiveCount}
            narrativeSpawnSuspected={narrativeSpawnSuspected}
            onDetected={onRequestAudit}
          />
        </div>
        )}

        {!officeSession && view === 'long-horizon' && (
          <LongHorizonPanel
            threadId={resumedThreadId ?? ''}
            streaming={streaming}
            pollFast={streaming || view === 'long-horizon'}
            onNavigateContextCategory={onNavigateContextCategory}
            onArchiveContext={onArchiveContext}
            archivePending={archivePending}
            canArchiveContext={canArchiveContext}
          />
        )}

        {/* Always mounted (hidden when inactive) so mermaid detection can auto-trigger the view */}
        <div
          className="flex min-h-0 flex-1 flex-col overflow-hidden"
          style={{ display: view === 'mermaid' ? undefined : 'none' }}
        >
          <MermaidPanel
            messages={messages}
            theme={theme}
            onDetected={view !== 'mermaid' ? () => onRequestMermaid?.() : undefined}
          />
        </div>

        {(view === 'settings' || view === 'system') && (
          <SettingsPanel
            runtimeConn={runtimeConn}
            desktopHost={desktopHost}
            apiKeyConfigured={apiKeyConfigured}
            platform={platform}
            theme={theme}
            onToggleTheme={onToggleTheme}
            streaming={streaming}
            onSettingsSaved={onSystemSettingsSaved}
            threadId={resumedThreadId}
          />
        )}

        {view === 'sandbox' && (
          <SandboxSettingsPanel desktopHost={desktopHost} platform={platform} streaming={streaming} />
        )}

        {view === 'lht-settings' && (
          <LhtSettingsPanel
            desktopHost={desktopHost}
            streaming={streaming}
            threadId={resumedThreadId}
          />
        )}

        {view === 'hooks' && (
          <HooksPanel desktopHost={desktopHost} streaming={streaming} />
        )}

        {view === 'schedule' && (
          <ScheduledAutomationsPanel
            runtimeConn={runtimeConn}
            streaming={streaming}
            runtimeSessionEstablished={runtimeSessionEstablished}
            onOpenTasks={onOpenTasks}
          />
        )}

        {view === 'about' && <AboutPanel />}

        {view === 'browser' && <BrowserPane desktopHost={desktopHost} />}

        {view === 'index' && !officeSession && (
          <IndexPanel
            workspace={workspaceRoot}
            onRebuild={onRebuildIndex}
            rebuilding={rebuildingIndex}
            rebuildError={rebuildIndexError}
            onRevealFile={revealWorkspaceFile}
          />
        )}
      </div>
    </aside>
    </div>
  );
}