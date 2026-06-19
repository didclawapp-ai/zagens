export const SESSION_STRIP_OPEN_STORAGE_KEY = 'zagens-session-strip-open';

export function readSessionStripOpen(defaultOpen = false): boolean {
  try {
    const raw = localStorage.getItem(SESSION_STRIP_OPEN_STORAGE_KEY);
    if (raw === 'true') {
      return true;
    }
    if (raw === 'false') {
      return false;
    }
  } catch {
    /* ignore */
  }
  return defaultOpen;
}

export function writeSessionStripOpen(open: boolean): void {
  try {
    localStorage.setItem(SESSION_STRIP_OPEN_STORAGE_KEY, String(open));
  } catch {
    /* ignore */
  }
}
