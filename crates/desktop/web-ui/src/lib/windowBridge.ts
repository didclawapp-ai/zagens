/** Tauri multi-window bridge (see docs/desktop/multi-window-plan.md). */

let windowLabel = 'dev';

export function getWindowLabel(): string {
  return windowLabel;
}

export function workspaceStorageKey(label: string = windowLabel): string {
  return `zagens-desktop-workspace:${label}`;
}

const LEGACY_ACTIVE_SESSION_STORAGE_KEY = 'zagens-desktop-active-session-id';

export function activeSessionStorageKey(label: string = windowLabel): string {
  return `zagens-desktop-active-session-id:${label}`;
}

export function loadStoredActiveSessionId(): string | null {
  try {
    const scopedKey = activeSessionStorageKey();
    let stored = localStorage.getItem(scopedKey)?.trim();
    if (!stored && windowLabel === 'main') {
      const legacy = localStorage.getItem(LEGACY_ACTIVE_SESSION_STORAGE_KEY)?.trim();
      if (legacy) {
        localStorage.setItem(scopedKey, legacy);
        localStorage.removeItem(LEGACY_ACTIVE_SESSION_STORAGE_KEY);
        stored = legacy;
      }
    }
    return stored && stored.length > 0 ? stored : null;
  } catch {
    return null;
  }
}

export function saveStoredActiveSessionId(sessionId: string): void {
  const id = sessionId.trim();
  if (!id) return;
  try {
    localStorage.setItem(activeSessionStorageKey(), id);
  } catch {
    /* ignore */
  }
}

export function clearStoredActiveSessionId(): void {
  try {
    localStorage.removeItem(activeSessionStorageKey());
    if (windowLabel === 'main') {
      localStorage.removeItem(LEGACY_ACTIVE_SESSION_STORAGE_KEY);
    }
  } catch {
    /* ignore */
  }
}

export async function initWindowContext(): Promise<{
  label: string;
  primaryWorkspace: string;
}> {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const label = await invoke<string>('get_window_label');
    windowLabel = label;
    let primaryWorkspace = '';
    try {
      primaryWorkspace = (await invoke<string>('get_window_workspace')).trim();
    } catch {
      /* main may not be registered yet in edge cases */
    }
    return { label, primaryWorkspace };
  } catch {
    windowLabel = 'dev';
    return { label: 'dev', primaryWorkspace: '' };
  }
}

export async function createAgentWindow(workspace?: string): Promise<string> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<string>('create_agent_window', {
    workspace: workspace?.trim() || undefined,
  });
}

export async function registerWindowThread(threadId: string): Promise<void> {
  if (!threadId.trim()) return;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('register_window_thread', { threadId: threadId.trim() });
    markThreadRegisteredLocally(threadId);
  } catch {
    /* Vite dev */
  }
}

const ownershipChecks = new Map<string, { owns: boolean; at: number }>();
const OWNERSHIP_TTL_MS = 250;

/** Optimistic sync peek — false only after a recent IPC miss. */
export function peekWindowOwnsThread(threadId: string): boolean {
  const tid = threadId.trim();
  if (!tid) return true;
  const hit = ownershipChecks.get(tid);
  if (!hit) return true;
  if (Date.now() - hit.at > OWNERSHIP_TTL_MS * 4) return true;
  return hit.owns;
}

export function markThreadRegisteredLocally(threadId: string): void {
  const tid = threadId.trim();
  if (!tid) return;
  ownershipChecks.set(tid, { owns: true, at: Date.now() });
}

export function invalidateThreadOwnership(threadId: string): void {
  ownershipChecks.delete(threadId.trim());
}

/** D10 — whether this webview may apply live SSE for `threadId` (IPC + short TTL cache). */
export async function windowOwnsThreadForStream(threadId: string): Promise<boolean> {
  const tid = threadId.trim();
  if (!tid) return true;
  const hit = ownershipChecks.get(tid);
  const now = Date.now();
  if (hit && now - hit.at < OWNERSHIP_TTL_MS) {
    return hit.owns;
  }
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const owns = await invoke<boolean>('thread_owned_by_window', { threadId: tid });
    ownershipChecks.set(tid, { owns, at: now });
    return owns;
  } catch {
    return true;
  }
}

export async function threadOwnedByWindow(threadId: string): Promise<boolean> {
  return windowOwnsThreadForStream(threadId);
}

export async function closeCurrentWindow(): Promise<void> {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('close_current_window');
  } catch {
    /* browser dev */
  }
}

export async function updateWindowTitle(workspace: string): Promise<void> {
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    const name =
      workspace.trim().split(/[/\\]/).filter(Boolean).pop() || 'Zagens';
    await getCurrentWindow().setTitle(`${name} — Zagens`);
  } catch {
    /* ignore */
  }
}
