import { describe, expect, it } from 'vitest';
import type { DiffEntry } from './diffEntries';
import { mergeDiffEntries, type PanelDiffEntry } from './mergeDiffEntries';
import {
  canToggleGitDiffMode,
  defaultGitDiffMode,
  displayDiffText,
  formatGitChangeSubtitle,
  gitDiffModeToStaged,
} from './diffPanelGit';
import type { WorkspaceChangeEntry } from '../../api/client';

function agent(partial: Partial<DiffEntry> & Pick<DiffEntry, 'id' | 'fileName'>): DiffEntry {
  return {
    diffText: '--- a\n+++ b\n+agent\n',
    toolName: 'edit_file',
    messageId: 'm1',
    status: 'done',
    added: 1,
    removed: 0,
    ...partial,
  };
}

function git(partial: Partial<WorkspaceChangeEntry> & Pick<WorkspaceChangeEntry, 'path'>): WorkspaceChangeEntry {
  return {
    index_status: ' ',
    worktree_status: 'M',
    kind: 'modified',
    ...partial,
  };
}

function panel(partial: Partial<PanelDiffEntry> & Pick<PanelDiffEntry, 'id' | 'path'>): PanelDiffEntry {
  return {
    label: partial.path,
    source: 'git',
    added: 0,
    removed: 0,
    ...partial,
  };
}

describe('mergeDiffEntries', () => {
  it('merges same path with git as primary source', () => {
    const gitChanges = [git({ path: 'src/a.rs' })];
    const agents = [agent({ id: 't1', fileName: 'src/a.rs' })];
    const merged = mergeDiffEntries(gitChanges, agents, 'all');
    expect(merged).toHaveLength(1);
    expect(merged[0].source).toBe('both');
    expect(merged[0].git).toBeDefined();
    expect(merged[0].agent).toBeDefined();
  });

  it('filters session-only', () => {
    const gitChanges = [git({ path: 'only-git.txt', index_status: '?', worktree_status: '?', kind: 'untracked' })];
    const agents = [agent({ id: 't2', fileName: 'only-agent.ts' })];
    const session = mergeDiffEntries(gitChanges, agents, 'session');
    expect(session.map((e) => e.path)).toEqual(['only-agent.ts']);
    const workspace = mergeDiffEntries(gitChanges, agents, 'workspace');
    expect(workspace.map((e) => e.path)).toEqual(['only-git.txt']);
  });
});

describe('diffPanelGit', () => {
  it('defaults to staged when only index changed', () => {
    expect(defaultGitDiffMode(git({ path: 'x', index_status: 'M', worktree_status: ' ' }))).toBe('staged');
  });

  it('prefers worktree when both index and worktree changed', () => {
    expect(defaultGitDiffMode(git({ path: 'x', index_status: 'M', worktree_status: 'M' }))).toBe('worktree');
    expect(canToggleGitDiffMode(git({ path: 'x', index_status: 'M', worktree_status: 'M' }))).toBe(true);
  });

  it('maps diff mode to staged query flag', () => {
    expect(gitDiffModeToStaged('staged')).toBe(true);
    expect(gitDiffModeToStaged('worktree')).toBe(false);
  });

  it('switches display between workspace and session for both entries', () => {
    const entry = panel({
      id: 'both:src/a.rs',
      path: 'src/a.rs',
      source: 'both',
      gitDiffText: '--- git\n',
      agentDiffText: '--- agent\n',
    });
    expect(displayDiffText(entry, 'workspace')).toBe('--- git\n');
    expect(displayDiffText(entry, 'session')).toBe('--- agent\n');
  });

  it('formats rename subtitle', () => {
    const entry = panel({
      id: 'git:new.rs',
      path: 'new.rs',
      git: git({ path: 'new.rs', kind: 'renamed', old_path: 'old.rs' }),
    });
    expect(formatGitChangeSubtitle(entry)).toBe('old.rs → new.rs');
  });
});
