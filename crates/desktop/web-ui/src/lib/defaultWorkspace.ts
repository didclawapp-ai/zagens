/** True when stored workspace should be replaced (e.g. `.` → process cwd / System32). */
export function isUnsafeComposerWorkspace(ws: string): boolean {
  const t = ws.trim().replace(/\\/g, '/').toLowerCase();
  if (!t || t === '.' || t === './') return true;
  if (t.includes('/system32') || t.endsWith('system32')) return true;
  if (t.includes('/syswow64')) return true;
  return false;
}

/** `<Documents>/DS Pick` from the desktop host; falls back if not in Tauri. */
export async function fetchDefaultComposerWorkspace(): Promise<string> {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const path = await invoke<string>('default_composer_workspace');
    if (typeof path === 'string' && path.trim().length > 0) {
      return path.trim();
    }
  } catch {
    /* browser build or invoke unavailable */
  }
  return '.';
}
