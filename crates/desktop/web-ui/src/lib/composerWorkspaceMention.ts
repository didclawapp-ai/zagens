import { normalizeWorkspaceRelPath } from './openWorkspaceFile';

/** Token inserted into Composer (TUI-style `@` file mention). */
export function formatWorkspaceMention(relPath: string, isDirectory = false): string {
  let rel = normalizeWorkspaceRelPath(relPath);
  if (!rel) return '';
  if (isDirectory && !rel.endsWith('/')) {
    rel = `${rel}/`;
  }
  if (/[\s#"]/.test(rel)) {
    return `@"${rel}"`;
  }
  return `@${rel}`;
}

/** Append a mention to existing composer text with spacing. */
export function appendWorkspaceMentionToText(current: string, mention: string): string {
  if (!mention) return current;
  const trimmed = current.trimEnd();
  if (!trimmed) return `${mention} `;
  const sep = /\s$/.test(current) ? '' : ' ';
  return `${current}${sep}${mention} `;
}
