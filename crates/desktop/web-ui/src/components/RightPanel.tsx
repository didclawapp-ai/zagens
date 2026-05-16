import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { invoke as InvokeFn } from '@tauri-apps/api/core';
import ApiKeyForm from './ApiKeyForm';
import McpPanel from './McpPanel';
import UsageDashboard from './UsageDashboard';
import AutomationPanel from './AutomationPanel';
import AgentPanel from './AgentPanel';
import RoutingPanel from './RoutingPanel';
import ChecklistPanel from './ChecklistPanel';
import MermaidPanel from './MermaidPanel';
import SettingsPanel from './SettingsPanel';
import type { AgentState } from '../types/agent';
import {
  PreviewContainer,
  PreviewDispatcher,
} from './preview';
import type { PreviewState } from './preview/types';
import type { RuntimeConnectionState } from '../api/client';
import { useT } from '../i18n';
import {
  browseThreadWorkspace,
  browseComposerWorkspace,
  getThreadSnapshots,
  restoreThreadSnapshot,
} from '../api/client';

export type RightPanelView =
  | 'workspace'
  | 'api-key'
  | 'settings'
  | 'system'
  | 'mcp'
  | 'usage'
  | 'tasks-skills'
  | 'agents'
  | 'routing'
  | 'checklist'
  | 'mermaid';

export type WorkspaceTabId = 'restore' | 'files' | 'rules';

type Theme = 'light' | 'dark';

const WORKSPACE_TAB_KEY = 'deepseek-desktop-right-workspace-tab';
const PANEL_WIDTH_KEY = 'deepseek-desktop-right-panel-width';
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
  /** Bumped when parent wants the workspace panel to show the Files tab (e.g. chat link). */
  focusFilesNonce: number;
  agentStates: AgentState[];
  /** Called when ChecklistPanel detects first data — parent switches view. */
  onRequestChecklist?: () => void;
  /** Chat messages — used by MermaidPanel to extract mermaid code blocks. */
  messages: { id: string; role: string; content: string }[];
  /** Called when MermaidPanel detects first mermaid block — parent switches view. */
  onRequestMermaid?: () => void;
  /** Called when user clicks collapse button in panel header. */
  onCollapse?: () => void;
}

const panelTitles: Record<RightPanelView, string> = {
  workspace: '工作台',
  'api-key': 'API Key',
  settings: '设置',
  system: '系统设置',
  mcp: 'MCP 服务器',
  usage: '用量仪表盘',
  'tasks-skills': '任务与技能',
  agents: '子代理',
  routing: '模型路由',
  checklist: 'Checklist',
  mermaid: 'Mermaid 图表',
};

function tabBtn(active: boolean) {
  return `flex-1 px-3 py-2.5 text-xs font-medium transition-colors border-b-2 -mb-px ${
    active
      ? 'border-accent text-accent bg-hover/50'
      : 'border-transparent text-t-text-muted hover:text-t-text hover:bg-hover/80'
  }`;
}

function joinRel(parent: string, name: string): string {
  const p = parent.trim();
  if (!p) return name;
  return `${p}/${name}`;
}

function pathBreadcrumbs(rel: string): { label: string; path: string }[] {
  const trimmed = rel.trim();
  const out: { label: string; path: string }[] = [{ label: '根目录', path: '' }];
  if (!trimmed) return out;
  const parts = trimmed.split('/').filter(Boolean);
  let acc = '';
  for (const part of parts) {
    acc = acc ? `${acc}/${part}` : part;
    out.push({ label: part, path: acc });
  }
  return out;
}

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
  focusFilesNonce,
  agentStates,
  onRequestChecklist,
  messages,
  onRequestMermaid,
  onCollapse,
}: Props) {
  const { t } = useT();
  const [workspaceTab, setWorkspaceTab] = useState<WorkspaceTabId>(() => {
    try {
      const s = sessionStorage.getItem(WORKSPACE_TAB_KEY);
      if (s === 'restore' || s === 'files' || s === 'rules') return s;
    } catch {
      /* ignore */
    }
    return 'files';
  });

  const [browseRelPath, setBrowseRelPath] = useState('');
  const [browseNonce, setBrowseNonce] = useState(0);
  const [browseWorkspace, setBrowseWorkspace] = useState<string | null>(null);
  const [browseEntries, setBrowseEntries] = useState<
    Array<{ name: string; kind: string; size?: number }>
  >([]);
  const [browseLoading, setBrowseLoading] = useState(false);
  const [browseError, setBrowseError] = useState<string | null>(null);

  const [snapshots, setSnapshots] = useState<
    Array<{ n: number; id: string; label: string; timestamp: number }>
  >([]);
  const [snapLoading, setSnapLoading] = useState(false);
  const [snapError, setSnapError] = useState<string | null>(null);
  const [restoreBusy, setRestoreBusy] = useState<number | null>(null);
  const [restoreMessage, setRestoreMessage] = useState<string | null>(null);

  const [pickRulesBody, setPickRulesBody] = useState('');
  const [pickRulesLoading, setPickRulesLoading] = useState(false);
  const [pickRulesSaving, setPickRulesSaving] = useState(false);
  const [pickRulesErr, setPickRulesErr] = useState<string | null>(null);
  const [pickRulesOk, setPickRulesOk] = useState<string | null>(null);

  const runtimeOk = runtimeConn === 'connected';

  useEffect(() => {
    try {
      sessionStorage.setItem(WORKSPACE_TAB_KEY, workspaceTab);
    } catch {
      /* ignore */
    }
  }, [workspaceTab]);

  useEffect(() => {
    setBrowseRelPath('');
    setBrowseNonce(0);
    setBrowseError(null);
    setBrowseEntries([]);
    setSnapshots([]);
    setSnapError(null);
    setRestoreMessage(null);
  }, [resumedThreadId, workspaceRoot]);

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
    if (view !== 'workspace' || workspaceTab !== 'files' || !runtimeOk) {
      return;
    }
    const root = workspaceRoot.trim();
    const hasThread = Boolean(resumedThreadId?.length);
    if (!hasThread && root.length === 0) {
      setBrowseLoading(false);
      setBrowseEntries([]);
      setBrowseWorkspace(null);
      return;
    }

    let cancelled = false;
    setBrowseLoading(true);
    setBrowseError(null);
    const req = hasThread
      ? browseThreadWorkspace(resumedThreadId!, browseRelPath || undefined)
      : browseComposerWorkspace(root, browseRelPath || undefined);
    void req
      .then((res) => {
        if (cancelled) return;
        setBrowseWorkspace(res.workspace);
        setBrowseEntries(res.entries ?? []);
      })
      .catch((e) => {
        if (cancelled) return;
        const err = e as Error & { status?: number };
        setBrowseError(err.message ?? String(e));
        setBrowseEntries([]);
      })
      .finally(() => {
        if (!cancelled) setBrowseLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [view, workspaceTab, resumedThreadId, workspaceRoot, browseRelPath, browseNonce, runtimeOk]);

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

  const crumbs = useMemo(() => pathBreadcrumbs(browseRelPath), [browseRelPath]);

  useEffect(() => {
    if (focusFilesNonce > 0) {
      setWorkspaceTab('files');
    }
  }, [focusFilesNonce]);

  const canBrowseComposerFiles =
    runtimeOk && (Boolean(resumedThreadId?.length) || workspaceRoot.trim().length > 0);

  const onOpenFileFromTree = useCallback(
    async (relPath: string, title: string) => {
      if (!runtimeOk) {
        return;
      }
      try {
        await openWorkspaceFile(relPath, title);
      } catch (e) {
        const err = e as Error & { status?: number };
        setBrowseError(err.message ?? String(e));
      }
    },
    [runtimeOk, openWorkspaceFile],
  );

  const onRestore = useCallback(
    async (n: number) => {
      if (!resumedThreadId || !runtimeOk) return;
      if (!confirm(`确定将工作区恢复到快照 #${n}？`)) return;
      setRestoreBusy(n);
      setRestoreMessage(null);
      try {
        const r = await restoreThreadSnapshot(resumedThreadId, n);
        setRestoreMessage(`已恢复：${r.label}（${r.id.slice(0, 8)}…）`);
        const list = await getThreadSnapshots(resumedThreadId, { limit: 50 });
        setSnapshots(list.snapshots ?? []);
        setBrowseNonce((n) => n + 1);
      } catch (e) {
        const err = e as Error & { status?: number };
        if (err.status === 403) {
          setRestoreMessage('需要在此线程上启用信任模式后才能恢复快照。');
        } else {
          setRestoreMessage(`恢复失败：${err.message ?? String(e)}`);
        }
      } finally {
        setRestoreBusy(null);
      }
    },
    [resumedThreadId, runtimeOk],
  );

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
      setPickRulesOk(t('workspaceRules.saved'));
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
    <div className="flex h-full max-h-screen shrink-0" aria-label="侧栏面板">
      <div
        role="separator"
        aria-orientation="vertical"
        aria-label="拖拽调整面板宽度"
        tabIndex={0}
        className={`w-1.5 shrink-0 cursor-col-resize touch-none select-none transition-colors bg-canvas ${
          panelResizing ? 'bg-canvas-alt' : 'hover:bg-hover'
        }`}
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
        className="flex min-w-0 shrink-0 flex-col border-l border-rail-edge bg-canvas overflow-hidden"
        style={{ width: panelWidth }}
      >
      <div className="flex shrink-0 items-center border-b border-divider px-4 py-3">
        <h2 className="flex-1 text-sm font-semibold text-t-text">{panelTitles[view]}</h2>
        {onCollapse && (
          <button
            type="button"
            onClick={onCollapse}
            className="ml-2 p-1 rounded text-t-text-muted hover:text-t-text hover:bg-hover transition-colors"
            title="收起面板"
          >
            <svg className="w-4 h-4" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5">
              <path d="M11 4l-6 4 6 4" strokeLinecap="round" strokeLinejoin="round"/>
            </svg>
          </button>
        )}
        {view === 'workspace' && desktopHost && (
          <button
            type="button"
            className="ml-auto px-2 py-1 text-[10px] text-t-text-muted hover:text-t-text hover:bg-hover rounded transition-colors"
            title="在文件管理器中打开工作区"
            onClick={async () => {
              try {
                const { invoke } = await import('@tauri-apps/api/core');
                await invoke('open_in_shell', { path: workspaceRoot });
              } catch {
                /* ignore */
              }
            }}
          >
            <svg viewBox="0 0 24 24" className="inline w-3.5 h-3.5 mr-1 stroke-current" style={{ fill: 'none', strokeWidth: 1.6 }}>
              <path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"/>
            </svg>
            打开文件夹
          </button>
        )}
      </div>

      <div className="flex-1 flex flex-col min-h-0 overflow-hidden text-sm text-t-text">
        {view === 'workspace' && (
          <>
            {!runtimeOk && (
              <p className="shrink-0 px-3 py-2 text-[11px] text-amber-text/90 border-b border-divider bg-amber-bg/30">
                本地运行时未连接；目录与快照 API 不可用。
              </p>
            )}
            {preview ? (
              <PreviewContainer title={preview.title} onClose={onClosePreview}>
                <PreviewDispatcher
                  state={preview}
                  onOpenWorkspaceRelativePath={(rel) => {
                    void openWorkspaceFile(rel).catch((err) => {
                      const e = err as Error & { status?: number };
                      setBrowseError(e.message ?? String(err));
                    });
                  }}
                />
              </PreviewContainer>
            ) : (
              <>
                <div
                  className="shrink-0 flex border-b border-divider bg-canvas-alt"
                  role="tablist"
                  aria-label="工作台分区"
                >
                  <button
                    type="button"
                    role="tab"
                    aria-selected={workspaceTab === 'restore'}
                    className={tabBtn(workspaceTab === 'restore')}
                    onClick={() => setWorkspaceTab('restore')}
                  >
                    恢复
                  </button>
                  <button
                    type="button"
                    role="tab"
                    aria-selected={workspaceTab === 'files'}
                    className={tabBtn(workspaceTab === 'files')}
                    onClick={() => setWorkspaceTab('files')}
                  >
                    工作区目录
                  </button>
                  <button
                    type="button"
                    role="tab"
                    aria-selected={workspaceTab === 'rules'}
                    className={tabBtn(workspaceTab === 'rules')}
                    onClick={() => setWorkspaceTab('rules')}
                  >
                    {t('workspaceRules.tab')}
                  </button>
                </div>

                <div className="flex-1 overflow-y-auto min-h-0" role="tabpanel">
                  {workspaceTab === 'restore' && (
                    <div className="p-4 space-y-3 text-xs text-t-text leading-relaxed">
                      <p className="text-t-text-muted">
                        与工作区 <strong className="text-t-text">side-git</strong> 快照同步，对应 TUI 的{' '}
                        <code className="rounded bg-canvas-alt px-1">/restore N</code>（N 与下列序号一致）。
                      </p>
                      {!resumedThreadId && (
                        <p className="text-amber-text/90">选择会话并恢复线程后，可加载快照列表。</p>
                      )}
                      {resumedThreadId && (
                        <p className="text-t-text-muted">
                          当前线程：<code className="text-[11px] break-all">{resumedThreadId.slice(0, 14)}…</code>
                          {!threadTrustMode && (
                            <span className="block mt-1">
                              恢复快照需要{' '}
                              <strong className="text-t-text">信任模式</strong>（仅本地运行时）。
                            </span>
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
                          启用信任模式
                        </button>
                      )}
                      {snapLoading && <p className="text-t-text-muted">加载快照…</p>}
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
                        <p className="text-t-text-muted">暂无快照记录。</p>
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
                                    #{s.n} · {s.label || '（无标题）'}
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
                                  title={!threadTrustMode ? '请先启用信任模式' : undefined}
                                  onClick={() => void onRestore(s.n)}
                                >
                                  {restoreBusy === s.n ? '…' : '恢复'}
                                </button>
                              </div>
                            </li>
                          ))}
                        </ul>
                      )}
                    </div>
                  )}

                  {workspaceTab === 'files' && (
                    <div className="flex flex-col min-h-0">
                      <div className="shrink-0 border-b border-divider px-4 py-3 space-y-1">
                        <p className="text-[11px] font-medium uppercase tracking-wide text-t-text-muted">
                          Composer 工作区
                        </p>
                        <p
                          className="text-xs font-mono text-t-text break-all leading-snug"
                          title={workspaceRoot}
                        >
                          {workspaceRoot || '（未设置）'}
                        </p>
                        {browseWorkspace && (
                          <p className="text-[10px] text-t-text-muted break-all" title={browseWorkspace}>
                            解析路径：{browseWorkspace}
                          </p>
                        )}
                      </div>
                      <div className="px-3 py-2 border-b border-divider flex flex-wrap items-center gap-1 text-[11px]">
                        {crumbs.map((c, i) => (
                          <span key={c.path || 'root'} className="flex items-center gap-1 min-w-0">
                            {i > 0 && <span className="text-t-text-muted">/</span>}
                            <button
                              type="button"
                              className="truncate max-w-[7rem] text-accent hover:underline"
                              title={c.path || '根目录'}
                              onClick={() => setBrowseRelPath(c.path)}
                            >
                              {c.label}
                            </button>
                          </span>
                        ))}
                      </div>
                      <div className="px-4 py-2 flex-1 min-h-0">
                        {browseLoading && <p className="text-xs text-t-text-muted">读取目录…</p>}
                        {browseError && (
                          <p className="text-xs text-red-300/90 break-words mb-2">{browseError}</p>
                        )}
                        {!canBrowseComposerFiles && (
                          <p className="text-xs text-amber-text/90">
                            请先连接本地运行时，并设置 Composer 工作区路径；或选择会话并恢复线程后再浏览目录。
                          </p>
                        )}
                        {canBrowseComposerFiles && !browseLoading && (
                          <ul className="space-y-0.5">
                            {browseEntries.map((ent) => {
                              const rel = joinRel(browseRelPath, ent.name);
                              const isDir = ent.kind === 'directory';
                              return (
                                <li key={rel}>
                                  {isDir ? (
                                    <button
                                      type="button"
                                      className="w-full text-left rounded-md px-2 py-1.5 text-xs text-t-text hover:bg-hover flex items-center gap-2"
                                      onClick={() => setBrowseRelPath(rel)}
                                    >
                                      <span className="text-t-text-muted">▸</span>
                                      <span className="font-medium truncate">{ent.name}</span>
                                    </button>
                                  ) : (
                                    <button
                                      type="button"
                                      className="w-full text-left rounded-md px-2 py-1.5 text-xs text-t-text hover:bg-hover flex items-center gap-2"
                                      onClick={() => void onOpenFileFromTree(rel, ent.name)}
                                    >
                                      <span className="text-t-text-muted">◇</span>
                                      <span className="truncate">{ent.name}</span>
                                      {ent.size != null && (
                                        <span className="text-[10px] text-t-text-muted ml-auto shrink-0">
                                          {ent.size > 1024
                                            ? `${(ent.size / 1024).toFixed(1)} KB`
                                            : `${ent.size} B`}
                                        </span>
                                      )}
                                    </button>
                                  )}
                                </li>
                              );
                            })}
                          </ul>
                        )}
                        {canBrowseComposerFiles &&
                          browseEntries.length === 0 &&
                          !browseLoading &&
                          !browseError && (
                          <p className="text-[11px] text-t-text-muted mt-2">此目录为空。</p>
                        )}
                      </div>
                    </div>
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
                </div>
              </>
            )}
          </>
        )}

        {view === 'api-key' && (
          <div className="p-4 overflow-y-auto">
            {!desktopHost && (
              <p className="mb-3 text-xs text-amber-text/90 leading-relaxed">
                当前未在 Tauri 桌面壳中运行，无法通过此处写入密钥；请在配置文件中手动设置或使用 CLI。
              </p>
            )}
            {desktopHost && apiKeyConfigured === false && (
              <p className="mb-3 text-xs text-amber-text/90 leading-relaxed">未检测到已保存的 DeepSeek API Key。</p>
            )}
            {desktopHost && (
              <ApiKeyForm
                onSaved={onSavedApiKey}
                className={!desktopHost ? 'pointer-events-none opacity-50' : ''}
              />
            )}
          </div>
        )}

        {view === 'mcp' && <McpPanel runtimeConn={runtimeConn} />}

        {view === 'usage' && <UsageDashboard runtimeConn={runtimeConn} />}

        {view === 'tasks-skills' && <AutomationPanel runtimeConn={runtimeConn} />}

        {view === 'agents' && <AgentPanel agents={agentStates} />}

        {view === 'routing' && <RoutingPanel runtimeConn={runtimeConn} />}

        {/* Always mounted (hidden when inactive) so polling can auto-trigger the view */}
        <div style={{ display: view === 'checklist' ? undefined : 'none' }}>
          <ChecklistPanel
            threadId={resumedThreadId ?? ''}
            onDetected={view !== 'checklist' ? () => onRequestChecklist?.() : undefined}
          />
        </div>

        {/* Always mounted (hidden when inactive) so mermaid detection can auto-trigger the view */}
        <div style={{ display: view === 'mermaid' ? undefined : 'none' }}>
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
          />
        )}
      </div>
    </aside>
    </div>
  );
}