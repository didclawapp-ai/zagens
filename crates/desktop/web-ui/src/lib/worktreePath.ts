/** True when the path looks like a managed git worktree under `.worktrees/`. */
export function isWorktreeWorkspacePath(path: string | undefined | null): boolean {
  const normalized = path?.replace(/\\/g, '/').trim() ?? '';
  if (!normalized) return false;
  return /(?:^|\/)\.worktrees(?:\/|$)/.test(normalized);
}

/** Short label for UI chips (worktree folder name or suffix). */
export function worktreeSessionLabel(path: string | undefined | null): string | null {
  const normalized = path?.replace(/\\/g, '/').trim() ?? '';
  if (!isWorktreeWorkspacePath(normalized)) return null;
  const idx = normalized.lastIndexOf('/.worktrees/');
  if (idx >= 0) {
    const tail = normalized.slice(idx + '/.worktrees/'.length);
    const name = tail.split('/')[0]?.trim();
    if (name) return name;
  }
  const parts = normalized.split('/');
  return parts[parts.length - 1]?.trim() || 'worktree';
}
