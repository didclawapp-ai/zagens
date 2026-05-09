import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import ApiKeyForm from './ApiKeyForm';
import {
  PreviewContainer,
  PreviewDispatcher,
  detectFileType,
  isBinaryFileType,
} from './preview';
import type { PreviewState } from './preview/types';
import type { RuntimeConnectionState } from '../api/client';
import {
  browseThreadWorkspace,
  browseComposerWorkspace,
  getThreadSnapshots,
  readThreadWorkspaceFile,
  readComposerWorkspaceFile,
  restoreThreadSnapshot,
} from '../api/client';

export type RightPanelView = 'workspace' | 'api-key' | 'settings';

export type WorkspaceTabId = 'restore' | 'files';

type Theme = 'light' | 'dark';

const WORKSPACE_TAB_KEY = 'deepseek-desktop-right-workspace-tab';
const PANEL_WIDTH_KEY = 'deepseek-desktop-right-panel-width';
const PANEL_MIN_PX = 260;
const PANEL_DEFAULT_PX = 320;

function clampPanelWidth(px: number): number {
  if (typeof window === 'undefined') {
    return Math.max(PANEL_MIN_PX, Math.round(px));
  }
  const cap = Math.min(720, Math.floor(window.innerWidth * 0.55));
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
  /** Current composer / thread workspace directory */
  workspaceRoot: string;
  /** Active runtime thread when session resumed — used for restore copy */
  resumedThreadId: string | null;
  /** From runtime thread detail — restore requires trust on server */
  threadTrustMode: boolean;
  onEnableTrust: () => Promise<void>;
}

const panelTitles: Record<RightPanelView, string> = {
  workspace: '工作台',
  'api-key': 'API Key',
  settings: '设置',
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
  workspaceRoot,
  resumedThreadId,
  threadTrustMode,
  onEnableTrust,
}: Props) {
  const [workspaceTab, setWorkspaceTab] = useState<WorkspaceTabId>(() => {
    try {
      const s = sessionStorage.getItem(WORKSPACE_TAB_KEY);
      if (s === 'restore' || s === 'files') return s;
    } catch {
      /* ignore */
    }
    return 'files';
  });

  const [preview, setPreview] = useState<PreviewState | null>(null);

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

  const openPreview = useCallback((p: PreviewState) => {
    setPreview(p);
  }, []);

  const closePreview = useCallback(() => {
    setPreview(null);
  }, []);

  const canBrowseComposerFiles =
    runtimeOk && (Boolean(resumedThreadId?.length) || workspaceRoot.trim().length > 0);

  const onOpenFile = useCallback(
    async (relPath: string, title: string) => {
      if (!runtimeOk) return;
      const root = workspaceRoot.trim();
      const fileType = detectFileType(title);

      if (resumedThreadId) {
        if (isBinaryFileType(fileType)) {
          try {
            const bin = await invoke<{
              mime_type: string;
              base64: string;
              size: number;
              truncated: boolean;
            }>('read_thread_workspace_binary', {
              threadId: resumedThreadId,
              relativePath: relPath,
            });
            openPreview({
              title,
              fileName: relPath.split('/').pop(),
              content: bin.base64,
              fileType,
              size: bin.size,
              mimeType: bin.mime_type,
              truncated: bin.truncated,
            });
          } catch (e) {
            const err = e as Error & { status?: number };
            setBrowseError(err.message ?? String(e));
          }
          return;
        }

        try {
          const file = await readThreadWorkspaceFile(resumedThreadId, relPath);
          openPreview({
            title,
            fileName: relPath.split('/').pop(),
            content: file.content,
            language: file.language_hint ?? undefined,
            fileType: detectFileType(relPath.split('/').pop(), file.language_hint),
          });
        } catch (e) {
          const err = e as Error & { status?: number };
          setBrowseError(err.message ?? String(e));
        }
        return;
      }

      if (!root) {
        setBrowseError('请先设置 Composer 工作区路径。');
        return;
      }

      if (isBinaryFileType(fileType)) {
        if (!desktopHost) {
          setBrowseError('二进制预览需使用桌面应用，或先发消息创建会话后再试。');
          return;
        }
        try {
          const bin = await invoke<{
            mime_type: string;
            base64: string;
            size: number;
            truncated: boolean;
          }>('read_workspace_binary_at_root', {
            workspaceRoot: root,
            relativePath: relPath,
          });
          openPreview({
            title,
            fileName: relPath.split('/').pop(),
            content: bin.base64,
            fileType,
            size: bin.size,
            mimeType: bin.mime_type,
            truncated: bin.truncated,
          });
        } catch (e) {
          const err = e as Error & { status?: number };
          setBrowseError(err.message ?? String(e));
        }
        return;
      }

      try {
        const file = await readComposerWorkspaceFile(root, relPath);
        openPreview({
          title,
          fileName: relPath.split('/').pop(),
          content: file.content,
          language: file.language_hint ?? undefined,
          fileType: detectFileType(relPath.split('/').pop(), file.language_hint),
        });
      } catch (e) {
        const err = e as Error & { status?: number };
        setBrowseError(err.message ?? String(e));
      }
    },
    [resumedThreadId, runtimeOk, workspaceRoot, desktopHost, openPreview],
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
        className={`w-1.5 shrink-0 cursor-col-resize touch-none select-none transition-colors ${
          panelResizing ? 'bg-accent/30' : 'hover:bg-accent/20'
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
        className="flex min-w-0 shrink-0 flex-col border-l border-divider bg-card overflow-hidden"
        style={{ width: panelWidth }}
      >
      <div className="flex shrink-0 items-center border-b border-divider px-4 py-3">
        <h2 className="flex-1 text-sm font-semibold text-t-text">{panelTitles[view]}</h2>
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
              <PreviewContainer title={preview.title} onClose={closePreview}>
                <PreviewDispatcher state={preview} />
              </PreviewContainer>
            ) : (
              <>
                <div
                  className="shrink-0 flex border-b border-divider bg-card"
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
                                      onClick={() => void onOpenFile(rel, ent.name)}
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

        {view === 'settings' && (
          <div className="p-4 space-y-4 overflow-y-auto">
            <p className="text-xs text-t-text-muted leading-relaxed">
              通用设置（主题、语言、默认模型等）将在此扩展。当前连接状态：
            </p>
            <dl className="space-y-2 text-xs">
              <div className="flex justify-between gap-2 py-1.5 border-b border-divider">
                <dt className="text-t-text-muted">本地运行时</dt>
                <dd className="text-t-text">
                  {runtimeConn === 'connected' && '已连接'}
                  {runtimeConn === 'checking' && '检测中…'}
                  {runtimeConn === 'offline' && '离线'}
                  {runtimeConn === 'auth_mismatch' && '令牌不一致'}
                </dd>
              </div>
              <div className="flex justify-between gap-2 py-1.5 border-b border-divider">
                <dt className="text-t-text-muted">主题</dt>
                <dd>
                  <button
                    type="button"
                    onClick={onToggleTheme}
                    className="text-accent hover:underline"
                  >
                    {theme === 'light' ? '浅色' : '暗色'}（点击切换）
                  </button>
                </dd>
              </div>
              <div className="flex justify-between gap-2 py-1.5">
                <dt className="text-t-text-muted">Tauri 桌面</dt>
                <dd className="text-t-text">{desktopHost ? '是' : '否（浏览器模式）'}</dd>
              </div>
            </dl>
          </div>
        )}
      </div>
    </aside>
    </div>
  );
}
