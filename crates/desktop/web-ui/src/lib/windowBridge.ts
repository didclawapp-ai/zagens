/** Tauri multi-window bridge (see docs/desktop/multi-window-plan.md). */

let windowLabel = 'dev';

export function getWindowLabel(): string {
  return windowLabel;
}

export function workspaceStorageKey(label: string = windowLabel): string {
  return `deepseek-desktop-workspace:${label}`;
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
  } catch {
    /* Vite dev */
  }
}

export async function threadOwnedByWindow(threadId: string): Promise<boolean> {
  if (!threadId.trim()) return true;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<boolean>('thread_owned_by_window', { threadId: threadId.trim() });
  } catch {
    return true;
  }
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
      workspace.trim().split(/[/\\]/).filter(Boolean).pop() || 'DS Pick';
    await getCurrentWindow().setTitle(`${name} — DS Pick`);
  } catch {
    /* ignore */
  }
}
