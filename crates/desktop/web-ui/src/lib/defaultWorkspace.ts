/** Strip Windows verbatim `\\?\` prefix before runtime API calls. */
export function normalizeWorkspaceForApi(ws: string): string {
  let t = ws.trim();
  if (t.startsWith('\\\\?\\UNC\\')) {
    t = `\\\\${t.slice('\\\\?\\UNC\\'.length)}`;
  } else if (t.startsWith('\\\\?\\')) {
    t = t.slice(4);
  }
  return t;
}

/** Compare workspace roots for sidebar filtering (case/separator tolerant on Windows). */
export function workspacesMatch(a: string, b: string): boolean {
  const left = normalizeWorkspaceForApi(a).replace(/\\/g, '/').replace(/\/+$/, '');
  const right = normalizeWorkspaceForApi(b).replace(/\\/g, '/').replace(/\/+$/, '');
  if (!left || !right) {
    return left === right;
  }
  return left.toLowerCase() === right.toLowerCase();
}

/** True when stored workspace should be replaced (e.g. `.` → process cwd / System32). */
export function isUnsafeComposerWorkspace(ws: string): boolean {
  const t = ws.trim().replace(/\\/g, '/').toLowerCase();
  if (!t || t === '.' || t === './') return true;
  if (t.includes('/system32') || t.endsWith('system32')) return true;
  if (t.includes('/syswow64')) return true;
  return false;
}

/** `<Documents>/Zagens` from the desktop host; falls back if not in Tauri. */
export async function fetchDefaultComposerWorkspace(): Promise<string> {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const path = await invoke<string>('default_composer_workspace');
    if (typeof path === 'string' && path.trim().length > 0) {
      return normalizeWorkspaceForApi(path);
    }
  } catch {
    /* browser build or invoke unavailable */
  }
  return '.';
}
