import { useEffect, useMemo, useRef, useState } from 'react';
import DiffCard from '../DiffCard';
import DiffLineStats from './DiffLineStats';
import { useT } from '../../i18n';
import { extractDiffEntries } from '../../lib/diff/diffEntries';
import {
  mergeDiffEntries,
  withGitDiffStats,
  type DiffFilter,
  type PanelDiffEntry,
} from '../../lib/diff/mergeDiffEntries';
import {
  canToggleGitDiffMode,
  defaultGitDiffMode,
  displayDiffText,
  formatGitChangeSubtitle,
  gitChangeFlags,
  gitDiffCacheKey,
  gitDiffModeToStaged,
  type DiffContentView,
  type GitDiffMode,
} from '../../lib/diff/diffPanelGit';
import { normalizeWorkspaceRelPath } from '../../lib/openWorkspaceFile';
import { IconFolder } from '../icons/FlatIcons';
import type { ToolCardModel } from '../ToolCard';
import {
  getWorkspaceChanges,
  getWorkspaceFileDiff,
  getWorkspaceStatus,
  type WorkspaceStatusResponse,
} from '../../api/client';
import DiffPullsSection from './DiffPullsSection';

interface Message {
  id: string;
  tools?: ToolCardModel[];
}

interface Props {
  messages: Message[];
  /** Thread / worktree workspace root (A0). */
  workspaceRoot?: string;
  /** First diff in the turn — parent switches to workspace / Diff tab */
  onDetected?: () => void;
  /** Reveal a workspace-relative path in the Files tab (no preview). */
  onRevealInFiles?: (relPath: string) => void;
  active: boolean;
  /** Bump to refresh git status/changes (e.g. turn end). */
  refreshNonce?: number;
}

type OutputFormat = 'side-by-side' | 'line-by-line';

const STATUS_POLL_MS = 12_000;

export default function DiffPanel({
  messages,
  workspaceRoot = '',
  onDetected,
  onRevealInFiles,
  active,
  refreshNonce = 0,
}: Props) {
  const { t } = useT();
  const firedRef = useRef(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [outputFormat, setOutputFormat] = useState<OutputFormat>('side-by-side');
  const [filter, setFilter] = useState<DiffFilter>('all');
  const [gitStatus, setGitStatus] = useState<WorkspaceStatusResponse | null>(null);
  const [gitChanges, setGitChanges] = useState<
    Awaited<ReturnType<typeof getWorkspaceChanges>>['changes']
  >([]);
  const [changesTruncated, setChangesTruncated] = useState(false);
  const [changesLoadError, setChangesLoadError] = useState<string | null>(null);
  const [panelEntries, setPanelEntries] = useState<PanelDiffEntry[]>([]);
  const [diffError, setDiffError] = useState<string | null>(null);
  const [bothContentView, setBothContentView] = useState<DiffContentView>('workspace');
  const [gitDiffMode, setGitDiffMode] = useState<GitDiffMode>('worktree');
  const changesDirtyRef = useRef(-1);
  const lastRefreshNonceRef = useRef(0);
  const fetchedGitPathsRef = useRef(new Set<string>());

  const agentEntries = useMemo(() => extractDiffEntries(messages), [messages]);
  const gitRepo = Boolean(gitStatus?.git_repo);

  const mergedBase = useMemo(
    () => mergeDiffEntries(gitRepo ? gitChanges : [], agentEntries, filter),
    [gitRepo, gitChanges, agentEntries, filter],
  );

  // Keep loaded gitDiffText when base list refreshes (do not keep loading on agent-only rows).
  useEffect(() => {
    setPanelEntries((prev) => {
      const prevByPath = new Map(prev.map((e) => [e.path, e]));
      return mergedBase.map((e) => {
        const old = prevByPath.get(e.path);
        if (old?.gitDiffText) {
          return {
            ...e,
            gitDiffText: old.gitDiffText,
            added: old.added,
            removed: old.removed,
            loading: false,
          };
        }
        if (old?.loading && (e.source === 'git' || e.source === 'both')) {
          return { ...e, loading: true };
        }
        return e;
      });
    });
  }, [mergedBase]);

  const selected =
    panelEntries.find((e) => e.id === selectedId) ??
    panelEntries[panelEntries.length - 1] ??
    null;
  const selectedPath = selected?.path ?? null;
  const selectedNeedsGit =
    gitRepo &&
    selected != null &&
    (selected.source === 'git' || selected.source === 'both') &&
    !gitChangeFlags(selected.git).isConflict;

  useEffect(() => {
    if (!selected) return;
    setBothContentView('workspace');
    setGitDiffMode(defaultGitDiffMode(selected.git));
  }, [selected?.id]);

  useEffect(() => {
    if (panelEntries.length === 0) {
      setSelectedId(null);
      return;
    }
    setSelectedId((prev) => {
      if (prev && panelEntries.some((e) => e.id === prev)) return prev;
      return panelEntries[panelEntries.length - 1]?.id ?? null;
    });
  }, [panelEntries]);

  useEffect(() => {
    if (!active || panelEntries.length === 0 || firedRef.current) return;
    firedRef.current = true;
    onDetected?.();
  }, [active, panelEntries.length, onDetected]);

  useEffect(() => {
    if (panelEntries.length === 0) {
      firedRef.current = false;
    }
  }, [panelEntries.length]);

  // Reset git state when workspace changes (avoid stale "双源" + stuck loading).
  useEffect(() => {
    changesDirtyRef.current = -1;
    fetchedGitPathsRef.current.clear();
    setGitChanges([]);
    setGitStatus(null);
    setChangesTruncated(false);
    setChangesLoadError(null);
    setDiffError(null);
    setPanelEntries((prev) =>
      prev.map((e) => ({ ...e, loading: false, gitDiffText: undefined })),
    );
  }, [workspaceRoot]);

  // Light status poll + on-demand changes when counts change.
  useEffect(() => {
    if (!active || !workspaceRoot.trim()) {
      return;
    }
    let cancelled = false;

    const refreshStatus = async () => {
      try {
        const status = await getWorkspaceStatus(workspaceRoot);
        if (cancelled) return;
        setGitStatus(status);
        const dirty = status.staged + status.unstaged + status.untracked;
        const force = refreshNonce !== lastRefreshNonceRef.current;
        if (force) lastRefreshNonceRef.current = refreshNonce;
        if (dirty !== changesDirtyRef.current || force) {
          changesDirtyRef.current = dirty;
          if (status.git_repo) {
            try {
              const changes = await getWorkspaceChanges(workspaceRoot);
              if (!cancelled) {
                setGitChanges(changes.changes ?? []);
                setChangesTruncated(Boolean(changes.truncated));
                setChangesLoadError(null);
              }
            } catch (err: unknown) {
              if (!cancelled) {
                setGitChanges([]);
                setChangesTruncated(false);
                setChangesLoadError(err instanceof Error ? err.message : String(err));
              }
            }
          } else {
            setGitChanges([]);
            setChangesTruncated(false);
            setChangesLoadError(null);
            fetchedGitPathsRef.current.clear();
          }
        }
      } catch {
        if (!cancelled) {
          setGitStatus(null);
          setGitChanges([]);
        }
      }
    };

    void refreshStatus();
    const id = window.setInterval(() => {
      if (document.visibilityState === 'visible') void refreshStatus();
    }, STATUS_POLL_MS);

    const onVis = () => {
      if (document.visibilityState === 'visible') void refreshStatus();
    };
    document.addEventListener('visibilitychange', onVis);

    return () => {
      cancelled = true;
      window.clearInterval(id);
      document.removeEventListener('visibilitychange', onVis);
    };
  }, [active, workspaceRoot, refreshNonce]);

  // Fetch file diff for git-backed entries only. Deps are primitives to avoid cancel/stuck races.
  useEffect(() => {
    if (!active || !selectedPath || !workspaceRoot.trim()) return;
    if (!selectedNeedsGit) return;
    const cacheKey = gitDiffCacheKey(selectedPath, gitDiffMode);
    if (fetchedGitPathsRef.current.has(cacheKey)) return;
    if (selected?.gitDiffText && selected.gitDiffMode === gitDiffMode) {
      fetchedGitPathsRef.current.add(cacheKey);
      return;
    }

    let cancelled = false;
    const path = selectedPath;
    const mode = gitDiffMode;
    setPanelEntries((prev) =>
      prev.map((e) => (e.path === path ? { ...e, loading: true } : e)),
    );
    setDiffError(null);

    void getWorkspaceFileDiff(workspaceRoot, path, gitDiffModeToStaged(mode))
      .then((res) => {
        if (cancelled) return;
        fetchedGitPathsRef.current.add(cacheKey);
        setPanelEntries((prev) =>
          prev.map((e) =>
            e.path === path
              ? withGitDiffStats(e, res.diff_text, mode, {
                  truncated: res.truncated,
                  binary: res.binary,
                  untracked: res.untracked,
                })
              : e,
          ),
        );
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        fetchedGitPathsRef.current.add(cacheKey);
        setDiffError(err instanceof Error ? err.message : String(err));
        setPanelEntries((prev) =>
          prev.map((e) => (e.path === path ? { ...e, loading: false } : e)),
        );
      });

    return () => {
      cancelled = true;
      setPanelEntries((prev) =>
        prev.map((e) =>
          e.path === path && e.loading && !(e.gitDiffText && e.gitDiffMode === mode)
            ? { ...e, loading: false }
            : e,
        ),
      );
    };
  }, [active, selectedPath, selectedNeedsGit, workspaceRoot, gitDiffMode, selected?.gitDiffText, selected?.gitDiffMode]);

  const statusBar = gitStatus?.git_repo ? (
    <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[10px] text-t-text-muted font-mono">
      <span className="text-t-text-secondary">{gitStatus.branch ?? 'HEAD'}</span>
      {gitStatus.ahead != null || gitStatus.behind != null ? (
        <span>
          {gitStatus.ahead != null && gitStatus.ahead > 0
            ? t('diff.gitAhead', { n: String(gitStatus.ahead) })
            : null}
          {gitStatus.behind != null && gitStatus.behind > 0
            ? t('diff.gitBehind', { n: String(gitStatus.behind) })
            : null}
        </span>
      ) : null}
      <span>
        {t('diff.gitCounts', {
          staged: String(gitStatus.staged),
          unstaged: String(gitStatus.unstaged),
          untracked: String(gitStatus.untracked),
        })}
      </span>
    </div>
  ) : workspaceRoot.trim() && gitStatus && !gitStatus.git_repo ? (
    <span className="text-[10px] text-t-text-muted">{t('diff.gitNotRepo')}</span>
  ) : null;

  const hasAny = panelEntries.length > 0;
  const selectedConflict = selected ? gitChangeFlags(selected.git).isConflict : false;
  const diffText = selected ? displayDiffText(selected, bothContentView) : null;
  const showBothToggle = selected?.source === 'both';
  const showGitModeToggle = selected?.git ? canToggleGitDiffMode(selected.git) : false;
  const diffMeta = selected?.gitDiffMeta;
  // Prefer showing agent/session diff immediately; never block on git fetch when we have content.
  const showLoading = Boolean(selected?.loading && !diffText && !selectedConflict);

  return (
    <div className="diff-panel flex min-h-0 flex-1 flex-col">
      <div className="shrink-0 flex flex-col gap-1.5 border-b border-divider bg-canvas-alt px-3 py-2">
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-[10px] font-medium uppercase tracking-wide text-t-text-muted">
            {t('diff.count', { count: String(panelEntries.length) })}
          </span>
          <div className="flex rounded-md border border-divider overflow-hidden text-[10px]">
            <button
              type="button"
              className={`px-2 py-1 transition-colors ${
                filter === 'all' ? 'bg-hover text-accent' : 'text-t-text-muted hover:bg-hover'
              }`}
              onClick={() => setFilter('all')}
            >
              {t('diff.filterAll')}
            </button>
            <button
              type="button"
              className={`px-2 py-1 border-l border-divider transition-colors ${
                filter === 'workspace' ? 'bg-hover text-accent' : 'text-t-text-muted hover:bg-hover'
              }`}
              onClick={() => setFilter('workspace')}
            >
              {t('diff.filterWorkspace')}
            </button>
            <button
              type="button"
              className={`px-2 py-1 border-l border-divider transition-colors ${
                filter === 'session' ? 'bg-hover text-accent' : 'text-t-text-muted hover:bg-hover'
              }`}
              onClick={() => setFilter('session')}
            >
              {t('diff.filterSession')}
            </button>
          </div>
          <div className="ml-auto flex rounded-md border border-divider overflow-hidden text-[10px]">
            <button
              type="button"
              className={`px-2 py-1 transition-colors ${
                outputFormat === 'side-by-side'
                  ? 'bg-hover text-accent'
                  : 'text-t-text-muted hover:bg-hover'
              }`}
              onClick={() => setOutputFormat('side-by-side')}
            >
              {t('diff.sideBySide')}
            </button>
            <button
              type="button"
              className={`px-2 py-1 border-l border-divider transition-colors ${
                outputFormat === 'line-by-line'
                  ? 'bg-hover text-accent'
                  : 'text-t-text-muted hover:bg-hover'
              }`}
              onClick={() => setOutputFormat('line-by-line')}
            >
              {t('diff.lineByLine')}
            </button>
          </div>
        </div>
        {statusBar}
        {changesTruncated ? (
          <p className="text-[10px] text-amber-text/90">{t('diff.changesTruncated')}</p>
        ) : null}
        {changesLoadError ? (
          <p className="text-[10px] text-amber-text/90">{t('diff.changesLoadError')}</p>
        ) : null}
      </div>

      {workspaceRoot.trim() && gitRepo ? (
        <DiffPullsSection
          workspaceRoot={workspaceRoot}
          active={active}
          refreshNonce={refreshNonce}
        />
      ) : null}

      {!hasAny ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center text-xs text-t-text-muted">
          {gitStatus &&
          gitStatus.git_repo &&
          gitStatus.staged + gitStatus.unstaged + gitStatus.untracked > 0 ? (
            <>
              <p>{t('diff.emptySessionHasGit')}</p>
              <p className="max-w-[16rem] text-[11px] leading-relaxed opacity-80">
                {t('diff.emptySessionHasGitHint')}
              </p>
            </>
          ) : (
            <>
              <p>{t('diff.empty')}</p>
              <p className="max-w-[16rem] text-[11px] leading-relaxed opacity-80">
                {t('diff.emptyHint')}
              </p>
            </>
          )}
        </div>
      ) : (
        <>
          <div className="shrink-0 max-h-[28%] overflow-y-auto border-b border-divider bg-canvas">
            <ul className="p-1.5 space-y-0.5" role="listbox" aria-label={t('diff.listLabel')}>
              {panelEntries.map((e) => {
                const isSel = e.id === selected?.id;
                const rel = normalizeWorkspaceRelPath(e.path);
                return (
                  <li key={e.id} className="flex items-stretch gap-0.5">
                    <button
                      type="button"
                      role="option"
                      aria-selected={isSel}
                      className={`min-w-0 flex-1 rounded-md px-2.5 py-1.5 text-left text-[11px] font-mono transition-colors ${
                        isSel
                          ? 'bg-accent-soft text-accent'
                          : 'text-t-text-secondary hover:bg-hover'
                      }`}
                      onClick={() => setSelectedId(e.id)}
                    >
                      <div className="flex min-w-0 items-center gap-2">
                        <span className="min-w-0 flex-1 truncate">{e.label}</span>
                        <SourceBadge source={e.source} />
                        <DiffLineStats added={e.added} removed={e.removed} />
                      </div>
                      <span className="block truncate text-[10px] opacity-70">
                        {formatGitChangeSubtitle(e) ?? e.agent?.toolName ?? 'git'}
                      </span>
                    </button>
                    {onRevealInFiles && rel ? (
                      <button
                        type="button"
                        className="shrink-0 rounded-md px-1.5 text-t-text-muted hover:text-accent hover:bg-hover transition-colors"
                        title={t('diff.showInFiles')}
                        onClick={(ev) => {
                          ev.stopPropagation();
                          onRevealInFiles(rel);
                        }}
                      >
                        <IconFolder className="size-3.5" />
                      </button>
                    ) : null}
                  </li>
                );
              })}
            </ul>
          </div>

          <div className="min-h-0 flex-1 overflow-hidden p-2 flex flex-col gap-2">
            {selected && onRevealInFiles ? (
              <div className="shrink-0 flex flex-wrap items-center justify-end gap-2">
                {showBothToggle ? (
                  <div className="mr-auto flex rounded-md border border-divider overflow-hidden text-[10px]">
                    <button
                      type="button"
                      className={`px-2 py-1 transition-colors ${
                        bothContentView === 'workspace'
                          ? 'bg-hover text-accent'
                          : 'text-t-text-muted hover:bg-hover'
                      }`}
                      onClick={() => setBothContentView('workspace')}
                    >
                      {t('diff.viewWorkspace')}
                    </button>
                    <button
                      type="button"
                      className={`px-2 py-1 border-l border-divider transition-colors ${
                        bothContentView === 'session'
                          ? 'bg-hover text-accent'
                          : 'text-t-text-muted hover:bg-hover'
                      }`}
                      onClick={() => setBothContentView('session')}
                    >
                      {t('diff.viewSession')}
                    </button>
                  </div>
                ) : null}
                {showGitModeToggle ? (
                  <div className="flex rounded-md border border-divider overflow-hidden text-[10px]">
                    <button
                      type="button"
                      className={`px-2 py-1 transition-colors ${
                        gitDiffMode === 'worktree'
                          ? 'bg-hover text-accent'
                          : 'text-t-text-muted hover:bg-hover'
                      }`}
                      onClick={() => setGitDiffMode('worktree')}
                    >
                      {t('diff.gitModeWorktree')}
                    </button>
                    <button
                      type="button"
                      className={`px-2 py-1 border-l border-divider transition-colors ${
                        gitDiffMode === 'staged'
                          ? 'bg-hover text-accent'
                          : 'text-t-text-muted hover:bg-hover'
                      }`}
                      onClick={() => setGitDiffMode('staged')}
                    >
                      {t('diff.gitModeStaged')}
                    </button>
                  </div>
                ) : null}
                <button
                  type="button"
                  className="inline-flex items-center gap-1 rounded-md border border-divider px-2 py-1 text-[10px] text-t-text-secondary hover:text-accent hover:bg-hover transition-colors"
                  onClick={() => {
                    const rel = normalizeWorkspaceRelPath(selected.path);
                    if (rel) onRevealInFiles(rel);
                  }}
                >
                  <IconFolder className="size-3" />
                  {t('diff.showInFiles')}
                </button>
              </div>
            ) : null}
            {selectedConflict ? (
              <p className="text-[11px] text-amber-text/90">{t('diff.conflictFile')}</p>
            ) : null}
            {diffMeta?.binary ? (
              <p className="text-[11px] text-t-text-muted">{t('diff.binaryFile')}</p>
            ) : null}
            {diffMeta?.truncated && !diffMeta.binary ? (
              <p className="text-[11px] text-amber-text/90">{t('diff.diffTruncated')}</p>
            ) : null}
            {diffError ? (
              <p className="text-[11px] text-amber-text/90">{diffError}</p>
            ) : null}
            <div className="min-h-0 flex-1 overflow-hidden">
              {showLoading ? (
                <p className="p-3 text-[11px] text-t-text-muted">{t('diff.loadingDiff')}</p>
              ) : diffText ? (
                <DiffCard
                  key={`${selected!.id}:${bothContentView}:${gitDiffMode}`}
                  diffText={diffText}
                  fileName={selected!.path}
                  outputFormat={outputFormat}
                  variant="panel"
                />
              ) : selected ? (
                <p className="p-3 text-[11px] text-t-text-muted">{t('diff.noHunks')}</p>
              ) : null}
            </div>
          </div>
        </>
      )}
    </div>
  );
}

function SourceBadge({ source }: { source: PanelDiffEntry['source'] }) {
  const { t } = useT();
  const label =
    source === 'both'
      ? t('diff.sourceBoth')
      : source === 'git'
        ? t('diff.sourceGit')
        : t('diff.sourceAgent');
  return (
    <span className="shrink-0 rounded px-1 py-0.5 text-[9px] uppercase tracking-wide bg-hover text-t-text-muted">
      {label}
    </span>
  );
}
