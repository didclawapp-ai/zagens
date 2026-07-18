import type { WorkspaceChangeEntry } from '../../api/client';
import type { PanelDiffEntry } from './mergeDiffEntries';

export type GitDiffMode = 'worktree' | 'staged';
export type DiffContentView = 'workspace' | 'session';

export function gitChangeFlags(git?: WorkspaceChangeEntry | null) {
  const idx = git?.index_status?.[0] ?? ' ';
  const wt = git?.worktree_status?.[0] ?? ' ';
  const hasStaged = idx !== ' ' && idx !== '?';
  const hasWorktree = wt !== ' ' && wt !== '?';
  const isUntracked = git?.kind === 'untracked';
  const isConflict = git?.kind === 'conflict';
  return { hasStaged, hasWorktree, isUntracked, isConflict };
}

/** Prefer worktree diff; fall back to staged when only index changed. */
export function defaultGitDiffMode(git?: WorkspaceChangeEntry | null): GitDiffMode {
  const { hasStaged, hasWorktree, isUntracked } = gitChangeFlags(git);
  if (isUntracked || hasWorktree) return 'worktree';
  if (hasStaged) return 'staged';
  return 'worktree';
}

export function gitDiffModeToStaged(mode: GitDiffMode): boolean {
  return mode === 'staged';
}

export function gitDiffCacheKey(path: string, mode: GitDiffMode): string {
  return `${path}:${mode}`;
}

export function canToggleGitDiffMode(git?: WorkspaceChangeEntry | null): boolean {
  const { hasStaged, hasWorktree, isUntracked } = gitChangeFlags(git);
  return hasStaged && hasWorktree && !isUntracked;
}

export function formatGitChangeSubtitle(entry: PanelDiffEntry): string | null {
  const g = entry.git;
  if (!g) return entry.kind ?? entry.agent?.toolName ?? null;
  if (g.old_path && (g.kind === 'renamed' || g.kind === 'copied')) {
    return `${g.old_path} → ${g.path}`;
  }
  return g.kind ?? null;
}

export function displayDiffText(
  entry: PanelDiffEntry,
  contentView: DiffContentView = 'workspace',
): string | null {
  if (entry.source === 'both') {
    if (contentView === 'session') {
      return entry.agentDiffText ?? entry.gitDiffText ?? null;
    }
    return entry.gitDiffText ?? entry.agentDiffText ?? null;
  }
  if (entry.gitDiffText) return entry.gitDiffText;
  if (entry.agentDiffText) return entry.agentDiffText;
  return null;
}
