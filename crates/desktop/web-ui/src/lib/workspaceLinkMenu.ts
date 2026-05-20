import { normalizeWorkspaceRelPath } from './openWorkspaceFile';

/** Extensions supported by `open_with_system_app` (must match `commands.rs`). */
const SYSTEM_OPENABLE_EXTS = new Set([
  'pdf',
  'png',
  'jpg',
  'jpeg',
  'gif',
  'svg',
  'webp',
  'bmp',
  'ico',
  'xlsx',
  'xls',
  'docx',
  'doc',
  'pptx',
  'ppt',
  'zip',
  'rar',
  '7z',
  'tar',
  'gz',
]);

export function isSystemOpenableFileName(fileName: string): boolean {
  const ext = (fileName.split('.').pop() ?? '').toLowerCase();
  return SYSTEM_OPENABLE_EXTS.has(ext);
}

/** Composer workspace root + normalized relative path → absolute path for shell / clipboard. */
export function workspaceAbsolutePath(workspaceRoot: string, relPath: string): string {
  const rel = normalizeWorkspaceRelPath(relPath);
  const base = workspaceRoot.trim().replace(/[\\/]+$/, '');
  if (!base) {
    return rel;
  }
  const sep = base.includes('\\') ? '\\' : '/';
  return `${base}${sep}${rel.replace(/\//g, sep)}`;
}
