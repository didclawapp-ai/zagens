import { useCallback, useEffect, useMemo, useState } from 'react';
import { browseComposerWorkspace, browseThreadWorkspace } from '../api/client';
import { useT } from '../i18n';
import type { PreviewState } from './preview/types';
import { normalizeWorkspaceRelPath } from '../lib/openWorkspaceFile';
import { formatWorkspaceFileError } from '../lib/workspaceFileOpenError';
import { useWorkspaceDirCache } from '../lib/useWorkspaceDirCache';
import {
  ancestorDirPaths,
  canOpenWithSystemApp,
  expandedDirsStorageKey,
  filterBrowseEntries,
  isDeniedDirName,
  joinWorkspaceRel,
  parentWorkspaceRel,
  pathBreadcrumbs,
  readExpandedDirs,
  readShowHiddenDirs,
  readWorkspaceDirViewMode,
  resolveBrowseAbsPath,
  workspaceRelPathsEqual,
  writeExpandedDirs,
  writeShowHiddenDirs,
  writeWorkspaceDirViewMode,
  type BrowseEntry,
  type WorkspaceDirViewMode,
} from '../lib/workspaceBrowse';
import {
  IconChevronRight,
  IconChevronUp,
  IconCopy,
  IconExternalFolder,
  IconEye,
  IconEyeOff,
  IconFolder,
  IconHome,
  IconList,
  IconRefresh,
  IconSearch,
  IconTree,
} from './icons/FlatIcons';
import { WorkspaceDirEntryRow } from './WorkspaceDirEntryRow';
import WorkspaceFileTree from './WorkspaceFileTree';

interface CtxEntry {
  absPath: string;
  relPath: string;
  name: string;
  kind: 'file' | 'directory';
  x: number;
  y: number;
}

export interface WorkspaceFilesPanelProps {
  /** Parent view is workspace and Files tab is selected. */
  active: boolean;
  workspaceRoot: string;
  resumedThreadId: string | null;
  runtimeOk: boolean;
  desktopHost: boolean;
  officeSession: boolean;
  preview: PreviewState | null;
  openWorkspaceFile: (relPath: string, title?: string) => Promise<void>;
  focusFilesNonce: number;
  focusFilesRelPath?: string | null;
  /** Bumped by parent (e.g. after snapshot restore) to reload directory listing. */
  externalRefreshNonce?: number;
}

const toolBtn =
  'inline-flex items-center justify-center gap-1 rounded-md px-2 py-1 text-[11px] text-t-text-secondary hover:text-t-text hover:bg-hover transition-colors disabled:opacity-40 disabled:pointer-events-none';

export default function WorkspaceFilesPanel({
  active,
  workspaceRoot,
  resumedThreadId,
  runtimeOk,
  desktopHost,
  officeSession,
  preview,
  openWorkspaceFile,
  focusFilesNonce,
  focusFilesRelPath,
  externalRefreshNonce = 0,
}: WorkspaceFilesPanelProps) {
  const { t } = useT();
  const [browseRelPath, setBrowseRelPath] = useState('');
  const [browseNonce, setBrowseNonce] = useState(0);
  const [browseWorkspace, setBrowseWorkspace] = useState<string | null>(null);
  const [browseEntries, setBrowseEntries] = useState<BrowseEntry[]>([]);
  const [browseLoading, setBrowseLoading] = useState(false);
  const [browseError, setBrowseError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState('');
  const [showHidden, setShowHidden] = useState(() => readShowHiddenDirs());
  const [viewMode, setViewMode] = useState<WorkspaceDirViewMode>(() => readWorkspaceDirViewMode());
  const [ctxMenu, setCtxMenu] = useState<CtxEntry | null>(null);

  const expandStorageKey = useMemo(
    () => expandedDirsStorageKey(workspaceRoot, resumedThreadId),
    [workspaceRoot, resumedThreadId],
  );
  const [expandedDirs, setExpandedDirs] = useState<Set<string>>(() =>
    readExpandedDirs(expandedDirsStorageKey(workspaceRoot, resumedThreadId)),
  );

  const treeCache = useWorkspaceDirCache({
    active: active && viewMode === 'tree',
    workspaceRoot,
    resumedThreadId,
    runtimeOk,
    refreshNonce: browseNonce + externalRefreshNonce,
  });

  const canBrowse =
    runtimeOk && (Boolean(resumedThreadId?.length) || workspaceRoot.trim().length > 0);

  const isTree = viewMode === 'tree';
  const resolvedWorkspace =
    (isTree ? treeCache.browseWorkspace : browseWorkspace) ?? workspaceRoot;
  const threadPathDiffers =
    Boolean(browseWorkspace?.trim()) &&
    browseWorkspace!.trim() !== workspaceRoot.trim();

  const previewRel = preview?.workspaceRelPath
    ? normalizeWorkspaceRelPath(preview.workspaceRelPath)
    : null;

  const crumbs = useMemo(
    () => pathBreadcrumbs(browseRelPath, t('workspaceFiles.breadcrumbRoot')),
    [browseRelPath, t],
  );

  const visibleEntries = useMemo(
    () => filterBrowseEntries(browseEntries, searchQuery, showHidden),
    [browseEntries, searchQuery, showHidden],
  );

  const hiddenFilteredCount = useMemo(() => {
    if (showHidden) return 0;
    const entries = isTree ? (treeCache.cache.get('') ?? []) : browseEntries;
    return entries.filter(
      (e) => e.kind === 'directory' && isDeniedDirName(e.name, false),
    ).length;
  }, [browseEntries, showHidden, isTree, treeCache.cache]);

  const absPath = useCallback(
    (rel: string) =>
      resolveBrowseAbsPath(
        rel,
        isTree ? treeCache.browseWorkspace : browseWorkspace,
        workspaceRoot,
      ),
    [browseWorkspace, workspaceRoot, isTree, treeCache.browseWorkspace],
  );

  useEffect(() => {
    setExpandedDirs(readExpandedDirs(expandStorageKey));
  }, [expandStorageKey]);

  const onToggleExpanded = useCallback(
    (dirRel: string) => {
      setExpandedDirs((prev) => {
        const next = new Set(prev);
        if (next.has(dirRel)) {
          next.delete(dirRel);
        } else {
          next.add(dirRel);
        }
        writeExpandedDirs(expandStorageKey, next);
        return next;
      });
    },
    [expandStorageKey],
  );

  const setViewModePersist = useCallback((mode: WorkspaceDirViewMode) => {
    setViewMode(mode);
    writeWorkspaceDirViewMode(mode);
  }, []);

  useEffect(() => {
    if (!isTree || !active || !canBrowse) return;
    const rel = focusFilesRelPath?.trim();
    if (!rel || focusFilesNonce <= 0) return;
    const toExpand = new Set(ancestorDirPaths(rel));
    const parent = parentWorkspaceRel(rel);
    if (parent) toExpand.add(parent);
    setExpandedDirs((prev) => {
      const next = new Set(prev);
      for (const p of toExpand) next.add(p);
      writeExpandedDirs(expandStorageKey, next);
      return next;
    });
    void treeCache.ensureLoaded('');
    for (const p of toExpand) {
      void treeCache.ensureLoaded(p);
    }
  }, [focusFilesNonce, focusFilesRelPath, isTree, active, canBrowse, expandStorageKey, treeCache]);

  useEffect(() => {
    if (!officeSession || !isTree || !active) return;
    setExpandedDirs((prev) => {
      const next = new Set(prev);
      next.add('deliverables');
      writeExpandedDirs(expandStorageKey, next);
      return next;
    });
    void treeCache.ensureLoaded('deliverables');
  }, [officeSession, isTree, active, expandStorageKey, treeCache]);

  useEffect(() => {
    setBrowseRelPath('');
    setBrowseNonce(0);
    setBrowseError(null);
    setBrowseEntries([]);
    setBrowseWorkspace(null);
    setSearchQuery('');
  }, [resumedThreadId, workspaceRoot]);

  useEffect(() => {
    if (!officeSession || !active) return;
    setBrowseRelPath((prev) => (prev === '' ? 'deliverables' : prev));
  }, [officeSession, active, workspaceRoot]);

  useEffect(() => {
    if (isTree || !active || !runtimeOk) return;
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
  }, [
    isTree,
    active,
    resumedThreadId,
    workspaceRoot,
    browseRelPath,
    browseNonce,
    externalRefreshNonce,
    runtimeOk,
  ]);

  useEffect(() => {
    if (isTree) return;
    if (focusFilesNonce <= 0) return;
    const rel = focusFilesRelPath?.trim();
    if (rel) {
      setBrowseRelPath(parentWorkspaceRel(rel));
    }
  }, [focusFilesNonce, focusFilesRelPath, isTree]);

  const listError = isTree ? treeCache.error : browseError;
  const listLoading = isTree ? treeCache.rootLoading : browseLoading;

  const handleRefresh = useCallback(() => {
    setBrowseNonce((n) => n + 1);
    if (isTree) {
      treeCache.clearCache();
      void treeCache.loadDir('', { force: true });
    }
  }, [isTree, treeCache]);

  useEffect(() => {
    if (!ctxMenu) return;
    const c = () => setCtxMenu(null);
    window.addEventListener('click', c, { once: true });
    return () => window.removeEventListener('click', c);
  }, [ctxMenu]);

  const onOpenFile = useCallback(
    async (relPath: string, title: string) => {
      if (!runtimeOk) return;
      try {
        await openWorkspaceFile(relPath, title);
      } catch (e) {
        const err = e as Error & { status?: number };
        const msg = formatWorkspaceFileError(e, t);
        setBrowseError(msg);
        treeCache.setError(msg);
      }
    },
    [runtimeOk, openWorkspaceFile, treeCache, t],
  );

  const ctxCopyAbs = useCallback(async () => {
    if (!ctxMenu) return;
    try {
      await navigator.clipboard.writeText(ctxMenu.absPath);
    } catch {
      /* ignore */
    }
    setCtxMenu(null);
  }, [ctxMenu]);

  const ctxCopyRel = useCallback(async () => {
    if (!ctxMenu) return;
    try {
      await navigator.clipboard.writeText(ctxMenu.relPath);
    } catch {
      /* ignore */
    }
    setCtxMenu(null);
  }, [ctxMenu]);

  const ctxOpenExplorer = useCallback(async () => {
    if (!ctxMenu) return;
    const target =
      ctxMenu.kind === 'directory'
        ? ctxMenu.absPath
        : ctxMenu.absPath.replace(/[\\/][^\\/]+$/, '');
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('open_in_shell', { path: target });
    } catch {
      /* ignore */
    }
    setCtxMenu(null);
  }, [ctxMenu]);

  const ctxSystemOpen = useCallback(async () => {
    if (!ctxMenu) return;
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('open_with_system_app', { path: ctxMenu.absPath });
    } catch {
      /* ignore */
    }
    setCtxMenu(null);
  }, [ctxMenu]);

  const ctxAddConv = useCallback(async () => {
    if (!ctxMenu) return;
    try {
      await openWorkspaceFile(ctxMenu.relPath || ctxMenu.name, ctxMenu.name);
    } catch (e) {
      setBrowseError(formatWorkspaceFileError(e, t));
    }
    setCtxMenu(null);
  }, [ctxMenu, openWorkspaceFile, t]);

  const openCurrentInShell = useCallback(async () => {
    if (!desktopHost) return;
    const path = absPath(isTree ? '' : browseRelPath);
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('open_in_shell', { path });
    } catch {
      /* ignore */
    }
  }, [desktopHost, absPath, browseRelPath, isTree]);

  const copyWorkspacePath = useCallback(async () => {
    const p = resolvedWorkspace.trim();
    if (!p) return;
    try {
      await navigator.clipboard.writeText(p);
    } catch {
      /* ignore */
    }
  }, [resolvedWorkspace]);

  const toggleShowHidden = useCallback(() => {
    setShowHidden((v) => {
      const next = !v;
      writeShowHiddenDirs(next);
      return next;
    });
  }, []);

  const openCtx = (e: { preventDefault: () => void; clientX: number; clientY: number }, ent: BrowseEntry, rel: string) => {
    e.preventDefault();
    setCtxMenu({
      absPath: absPath(rel),
      relPath: rel,
      name: ent.name,
      kind: ent.kind === 'directory' ? 'directory' : 'file',
      x: Math.min(e.clientX, window.innerWidth - 200),
      y: e.clientY,
    });
  };

  return (
    <div className="flex flex-col min-h-0 flex-1">
      <div className="shrink-0 border-b border-divider px-3 py-2.5 space-y-1.5">
        <div className="flex items-start gap-2 min-w-0">
          <IconFolder className="size-4 mt-0.5 text-accent/80 shrink-0" />
          <div className="min-w-0 flex-1">
            <p className="text-[11px] font-medium uppercase tracking-wide text-t-text-muted">
              {t('workspaceFiles.workspaceLabel')}
            </p>
            <p
              className="text-xs font-mono text-t-text break-all leading-snug"
              title={resolvedWorkspace || undefined}
            >
              {workspaceRoot.trim() || t('workspaceFiles.workspaceUnset')}
            </p>
            {threadPathDiffers && browseWorkspace && (
              <p
                className="text-[10px] text-t-text-muted break-all mt-0.5"
                title={browseWorkspace}
              >
                {t('workspaceFiles.threadWorkspace', { path: browseWorkspace })}
              </p>
            )}
          </div>
          {resolvedWorkspace.trim() && (
            <button
              type="button"
              className={`${toolBtn} shrink-0`}
              title={t('workspaceFiles.copyPath')}
              onClick={() => void copyWorkspacePath()}
            >
              <IconCopy className="size-3.5" />
            </button>
          )}
        </div>
      </div>

      <div className="shrink-0 flex flex-wrap items-center gap-0.5 px-2 py-1.5 border-b border-divider">
        {!isTree && (
          <button
            type="button"
            className={toolBtn}
            disabled={!browseRelPath || !canBrowse}
            title={t('workspaceFiles.goUp')}
            onClick={() => setBrowseRelPath(parentWorkspaceRel(browseRelPath))}
          >
            <IconChevronUp className="size-3.5" />
            <span className="hidden sm:inline">{t('workspaceFiles.goUp')}</span>
          </button>
        )}
        <button
          type="button"
          className={toolBtn}
          disabled={!canBrowse}
          title={t('workspaceFiles.refresh')}
          onClick={handleRefresh}
        >
          <IconRefresh className="size-3.5" />
          <span className="hidden sm:inline">{t('workspaceFiles.refresh')}</span>
        </button>
        {desktopHost && (
          <button
            type="button"
            className={toolBtn}
            disabled={!canBrowse}
            title={t('workspaceFiles.openInExplorer')}
            onClick={() => void openCurrentInShell()}
          >
            <IconExternalFolder className="size-3.5" />
            <span className="hidden sm:inline">{t('workspaceFiles.openInExplorer')}</span>
          </button>
        )}
        <div className="ml-auto flex items-center gap-0.5 rounded-md border border-divider p-0.5">
          <button
            type="button"
            className={`${toolBtn} px-1.5 py-0.5 ${!isTree ? 'bg-hover text-t-text' : ''}`}
            title={t('workspaceFiles.viewFlat')}
            aria-pressed={!isTree}
            onClick={() => setViewModePersist('flat')}
          >
            <IconList className="size-3.5" />
          </button>
          <button
            type="button"
            className={`${toolBtn} px-1.5 py-0.5 ${isTree ? 'bg-hover text-t-text' : ''}`}
            title={t('workspaceFiles.viewTree')}
            aria-pressed={isTree}
            onClick={() => setViewModePersist('tree')}
          >
            <IconTree className="size-3.5" />
          </button>
        </div>
        <button
          type="button"
          className={toolBtn}
          title={showHidden ? t('workspaceFiles.hideHidden') : t('workspaceFiles.showHidden')}
          onClick={toggleShowHidden}
        >
          {showHidden ? <IconEye className="size-3.5" /> : <IconEyeOff className="size-3.5" />}
        </button>
      </div>

      <div className="shrink-0 px-2 py-1.5 border-b border-divider">
        <label className="flex items-center gap-2 rounded-md border border-divider bg-canvas-alt/40 px-2 py-1">
          <IconSearch className="size-3.5 text-t-text-muted shrink-0" />
          <input
            type="search"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder={t('workspaceFiles.searchPlaceholder')}
            className="flex-1 min-w-0 bg-transparent text-xs text-t-text placeholder:text-t-text-muted outline-none"
            disabled={!canBrowse}
          />
        </label>
      </div>

      {!isTree && (
        <div className="shrink-0 px-2 py-1.5 border-b border-divider overflow-x-auto">
          <nav
            className="flex items-center gap-0.5 text-[11px] min-w-max"
            aria-label={t('workspaceFiles.breadcrumbAria')}
          >
            {crumbs.map((c, i) => (
              <span key={c.path || 'root'} className="flex items-center gap-0.5 shrink-0">
                {i > 0 && <IconChevronRight className="size-3 text-t-text-muted" />}
                <button
                  type="button"
                  className={`rounded px-1 py-0.5 hover:bg-hover hover:text-accent transition-colors ${
                    i === crumbs.length - 1 ? 'font-medium text-t-text' : 'text-accent'
                  }`}
                  title={c.path || t('workspaceFiles.breadcrumbRoot')}
                  onClick={() => setBrowseRelPath(c.path)}
                >
                  {i === 0 ? (
                    <span className="inline-flex items-center gap-1">
                      <IconHome className="size-3" />
                      {c.label}
                    </span>
                  ) : (
                    c.label
                  )}
                </button>
              </span>
            ))}
          </nav>
        </div>
      )}

      <div className="flex-1 min-h-0 overflow-y-auto px-2 py-2">
        {listLoading && (
          <p className="text-xs text-t-text-muted px-1">{t('workspaceFiles.loading')}</p>
        )}
        {listError && (
          <p className="text-xs text-red-300/90 break-words mb-2 px-1">{listError}</p>
        )}
        {!canBrowse && (
          <p className="text-xs text-amber-text/90 px-1">{t('workspaceFiles.needRuntime')}</p>
        )}
        {canBrowse && !listLoading && (
          <>
            {hiddenFilteredCount > 0 && !searchQuery.trim() && (
              <p className="text-[10px] text-t-text-muted px-1 mb-1.5">
                {t('workspaceFiles.hiddenFiltered', { count: String(hiddenFilteredCount) })}
              </p>
            )}
            {isTree ? (
              <WorkspaceFileTree
                cache={treeCache.cache}
                loadingPaths={treeCache.loadingPaths}
                expanded={expandedDirs}
                onToggleExpanded={onToggleExpanded}
                showHidden={showHidden}
                searchQuery={searchQuery}
                previewRel={previewRel}
                ensureLoaded={treeCache.ensureLoaded}
                onOpenFile={(rel, title) => void onOpenFile(rel, title)}
                onOpenContextMenu={openCtx}
              />
            ) : (
              <ul className="space-y-0.5">
                {visibleEntries.map((ent) => {
                  const rel = joinWorkspaceRel(browseRelPath, ent.name);
                  const isDir = ent.kind === 'directory';
                  const isPreviewed =
                    !isDir && previewRel != null && workspaceRelPathsEqual(rel, previewRel);
                  return (
                    <li key={rel}>
                      <WorkspaceDirEntryRow
                        ent={ent}
                        rel={rel}
                        depth={0}
                        isPreviewed={isPreviewed}
                        leading={
                          isDir ? (
                            <IconChevronRight className="size-3 text-t-text-muted shrink-0" />
                          ) : (
                            <span className="size-3 shrink-0" />
                          )
                        }
                        sensitiveHint={t('workspaceFiles.sensitiveHint')}
                        addToChatTitle={t('workspaceFiles.addToChat')}
                        onPrimaryClick={() =>
                          isDir ? setBrowseRelPath(rel) : void onOpenFile(rel, ent.name)
                        }
                        onContextMenu={(e) => openCtx(e, ent, rel)}
                        onAddToChat={() => void onOpenFile(rel, ent.name)}
                      />
                    </li>
                  );
                })}
              </ul>
            )}
            {!isTree && visibleEntries.length === 0 && !listError && (
              <p className="text-[11px] text-t-text-muted mt-2 px-1">
                {searchQuery.trim()
                  ? t('workspaceFiles.noSearchMatch')
                  : t('workspaceFiles.emptyDir')}
              </p>
            )}
          </>
        )}
      </div>

      {ctxMenu && (
        <div
          className="fixed z-50 min-w-[188px] rounded-lg border border-divider bg-canvas py-1 shadow-lg"
          style={{ left: `${ctxMenu.x}px`, top: `${ctxMenu.y}px` }}
          role="menu"
        >
          <div
            className="px-3 py-1.5 text-[11px] font-medium text-t-text-muted truncate border-b border-divider"
            title={ctxMenu.name}
          >
            {ctxMenu.name}
          </div>
          <button
            type="button"
            className="w-full text-left px-3 py-1.5 text-xs text-t-text hover:bg-hover"
            onClick={ctxCopyAbs}
          >
            {t('workspaceFiles.ctxCopyAbs')}
          </button>
          <button
            type="button"
            className="w-full text-left px-3 py-1.5 text-xs text-t-text hover:bg-hover"
            onClick={ctxCopyRel}
          >
            {t('workspaceFiles.ctxCopyRel')}
          </button>
          <button
            type="button"
            className="w-full text-left px-3 py-1.5 text-xs text-t-text hover:bg-hover"
            onClick={ctxAddConv}
          >
            {t('workspaceFiles.addToChat')}
          </button>
          <div className="border-t border-divider my-0.5" />
          <button
            type="button"
            className="w-full text-left px-3 py-1.5 text-xs text-t-text hover:bg-hover"
            onClick={ctxOpenExplorer}
          >
            {t('workspaceFiles.ctxOpenExplorer')}
          </button>
          {ctxMenu.kind === 'file' && canOpenWithSystemApp(ctxMenu.name) && (
            <button
              type="button"
              className="w-full text-left px-3 py-1.5 text-xs text-t-text hover:bg-hover"
              onClick={ctxSystemOpen}
            >
              {t('workspaceFiles.ctxSystemOpen')}
            </button>
          )}
        </div>
      )}
    </div>
  );
}
