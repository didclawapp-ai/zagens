/** HTML workspace preview prefs (localStorage). */

const ALLOW_SCRIPTS_KEY = 'zagens-desktop-html-preview-allow-scripts';

/**
 * When true, iframe sandbox is `allow-scripts` (without `allow-same-origin`).
 * Default true so rewritten HTML pages with inline/external scripts can run.
 */
export function readHtmlPreviewAllowScriptsPref(): boolean {
  try {
    const raw = localStorage.getItem(ALLOW_SCRIPTS_KEY);
    if (raw == null) return true;
    return raw === '1';
  } catch {
    return true;
  }
}

export function writeHtmlPreviewAllowScriptsPref(on: boolean): void {
  try {
    localStorage.setItem(ALLOW_SCRIPTS_KEY, on ? '1' : '0');
  } catch {
    /* ignore */
  }
}
