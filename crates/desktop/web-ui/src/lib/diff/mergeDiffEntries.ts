import { countUnifiedDiffLines, type DiffEntry } from './diffEntries';
import { normalizeWorkspaceRelPath } from '../openWorkspaceFile';
import type { WorkspaceChangeEntry } from '../../api/client';
import type { GitDiffMode } from './diffPanelGit';

export type DiffFilter = 'all' | 'workspace' | 'session';

export type PanelDiffSource = 'git' | 'agent' | 'both';

export interface PanelDiffEntry {
  id: string;
  /** Workspace-relative path used for selection / reveal. */
  path: string;
  label: string;
  source: PanelDiffSource;
  kind?: string;
  /** Live git unified diff (preferred when present). */
  gitDiffText?: string;
  /** Historical agent tool-card diff. */
  agentDiffText?: string;
  agent?: DiffEntry;
  git?: WorkspaceChangeEntry;
  gitDiffMode?: GitDiffMode;
  gitDiffMeta?: {
    truncated: boolean;
    binary: boolean;
    untracked: boolean;
  };
  added: number;
  removed: number;
  loading?: boolean;
}

function pathKey(path: string): string {
  return normalizeWorkspaceRelPath(path) ?? path.replace(/\\/g, '/');
}

function baseName(path: string): string {
  return path.split(/[/\\]/).pop() ?? path;
}

/** Merge git changes with session agent diffs; git content wins for display. */
export function mergeDiffEntries(
  gitChanges: WorkspaceChangeEntry[],
  agentEntries: DiffEntry[],
  filter: DiffFilter,
): PanelDiffEntry[] {
  const byPath = new Map<string, PanelDiffEntry>();

  for (const g of gitChanges) {
    const key = pathKey(g.path);
    byPath.set(key, {
      id: `git:${key}`,
      path: key,
      label: baseName(key),
      source: 'git',
      kind: g.kind,
      git: g,
      added: 0,
      removed: 0,
    });
  }

  for (const a of agentEntries) {
    const key = pathKey(a.fileName);
    const existing = byPath.get(key);
    if (existing) {
      existing.source = 'both';
      existing.agent = a;
      existing.agentDiffText = a.diffText;
      if (!existing.gitDiffText) {
        existing.added = a.added;
        existing.removed = a.removed;
      }
    } else {
      byPath.set(key, {
        id: `agent:${a.id}`,
        path: key,
        label: baseName(key),
        source: 'agent',
        agent: a,
        agentDiffText: a.diffText,
        added: a.added,
        removed: a.removed,
      });
    }
  }

  let list = [...byPath.values()];
  if (filter === 'workspace') {
    list = list.filter((e) => e.source === 'git' || e.source === 'both');
  } else if (filter === 'session') {
    list = list.filter((e) => e.source === 'agent' || e.source === 'both');
  }

  list.sort((a, b) => a.path.localeCompare(b.path));
  return list;
}

export function withGitDiffStats(
  entry: PanelDiffEntry,
  diffText: string,
  mode: GitDiffMode,
  meta?: PanelDiffEntry['gitDiffMeta'],
): PanelDiffEntry {
  const { added, removed } = countUnifiedDiffLines(diffText);
  return {
    ...entry,
    gitDiffText: diffText,
    gitDiffMode: mode,
    gitDiffMeta: meta,
    added,
    removed,
    loading: false,
  };
}
