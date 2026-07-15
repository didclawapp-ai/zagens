import { invoke } from '@tauri-apps/api/core';
import { isAllowedExternalUrl } from './openExternalUrl';

export const OPEN_BROWSER_PANE_EVENT = 'zagens:open-browser-pane';

/** Ask the shell to show the Browser right-panel view. */
export function requestOpenBrowserPane(): void {
  window.dispatchEvent(new CustomEvent(OPEN_BROWSER_PANE_EVENT));
}

function isLoopbackHttpUrl(url: string): boolean {
  try {
    const u = new URL(url.trim());
    if (u.protocol !== 'http:' && u.protocol !== 'https:') return false;
    const h = u.hostname.replace(/^\[|\]$/g, '').toLowerCase();
    return h === '127.0.0.1' || h === 'localhost' || h === '::1';
  } catch {
    return false;
  }
}

/**
 * Open a URL in the desktop Browser pane (human actor). Creates the host if needed.
 * Always requests the Browser pane so the user can see the navigation.
 */
export async function openInAppBrowser(url: string): Promise<void> {
  const trimmed = url.trim();
  if (!trimmed) return;
  if (trimmed !== 'about:blank' && !isAllowedExternalUrl(trimmed) && !isLoopbackHttpUrl(trimmed)) {
    console.warn('[openInAppBrowser] blocked unsafe url:', trimmed);
    return;
  }
  requestOpenBrowserPane();
  try {
    await invoke('browser_navigate', { args: { url: trimmed, actor: 'human' } });
  } catch {
    await invoke('browser_create', {
      args: { mode: 'auto', url: trimmed },
    });
  }
}
