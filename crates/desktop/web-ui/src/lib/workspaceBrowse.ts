/** Workspace directory browse helpers (DS Pick workbench → Files tab). */

export const WORKSPACE_DIR_SHOW_HIDDEN_KEY = 'ds-pick-dir-show-hidden';

export type BrowseEntry = { name: string; kind: string; size?: number };

/** Default-hidden directory names (noise in monorepos). */
export const WORKSPACE_DIR_DENYLIST = new Set([
  'node_modules',
  'target',
  'vendor',
  'dist',
  'build',
  '.git',
  '.cursor',
  '.deepseek',
  '.trae',
  '.claude',
]);

const SENSITIVE_NAME_RE =
  /^\.env(\.|$)|credentials|secret|\.pem$|\.key$|id_rsa|\.p12$|\.pfx$/i;

export function joinWorkspaceRel(parent: string, name: string): string {
  const p = parent.trim();
  if (!p) return name;
  return `${p}/${name}`;
}

export function parentWorkspaceRel(rel: string): string {
  const trimmed = rel.trim().replace(/\\/g, '/').replace(/\/+$/, '');
  if (!trimmed) return '';
  const idx = trimmed.lastIndexOf('/');
  return idx < 0 ? '' : trimmed.slice(0, idx);
}

export function workspaceRelPathsEqual(a: string, b: string): boolean {
  return a.trim().replace(/\\/g, '/') === b.trim().replace(/\\/g, '/');
}

export function pathBreadcrumbs(
  rel: string,
  rootLabel: string,
): { label: string; path: string }[] {
  const trimmed = rel.trim().replace(/\\/g, '/');
  const out: { label: string; path: string }[] = [{ label: rootLabel, path: '' }];
  if (!trimmed) return out;
  const parts = trimmed.split('/').filter(Boolean);
  let acc = '';
  for (const part of parts) {
    acc = acc ? `${acc}/${part}` : part;
    out.push({ label: part, path: acc });
  }
  return out;
}

export function formatBrowseEntrySize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function readShowHiddenDirs(): boolean {
  try {
    return localStorage.getItem(WORKSPACE_DIR_SHOW_HIDDEN_KEY) === '1';
  } catch {
    return false;
  }
}

export function writeShowHiddenDirs(show: boolean): void {
  try {
    localStorage.setItem(WORKSPACE_DIR_SHOW_HIDDEN_KEY, show ? '1' : '0');
  } catch {
    /* ignore */
  }
}

export function isDeniedDirName(name: string, showHidden: boolean): boolean {
  if (showHidden) return false;
  return WORKSPACE_DIR_DENYLIST.has(name);
}

export function isSensitiveEntryName(name: string): boolean {
  return SENSITIVE_NAME_RE.test(name);
}

export function filterBrowseEntries(
  entries: BrowseEntry[],
  query: string,
  showHidden: boolean,
): BrowseEntry[] {
  const q = query.trim().toLowerCase();
  return entries.filter((ent) => {
    if (ent.kind === 'directory' && isDeniedDirName(ent.name, showHidden)) {
      return false;
    }
    if (!q) return true;
    return ent.name.toLowerCase().includes(q);
  });
}

export function resolveBrowseAbsPath(
  rel: string,
  browseWorkspace: string | null,
  workspaceRoot: string,
): string {
  const base = (browseWorkspace ?? workspaceRoot).replace(/[\\/]+$/, '');
  if (!base) return rel;
  const sep = base.includes('\\') ? '\\' : '/';
  if (!rel) return base;
  return `${base}${sep}${rel.replace(/\//g, sep)}`;
}

const SYS_OPEN_EXTS = new Set([
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

export function canOpenWithSystemApp(fileName: string): boolean {
  return SYS_OPEN_EXTS.has((fileName.split('.').pop() ?? '').toLowerCase());
}

export function normalizePathsForCompare(p: string): string {
  return p.trim().replace(/\\/g, '/').replace(/^\/+/, '');
}
