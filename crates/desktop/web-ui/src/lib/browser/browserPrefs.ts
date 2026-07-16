/** Browser pane preferences (localStorage). */

export type BrowserEmbedMode = 'auto' | 'embedded' | 'windowed';

const MODE_KEY = 'zagens-desktop-browser-mode';
const PERSIST_KEY = 'zagens-desktop-browser-persist-profile';
const DESTROY_ON_CLOSE_KEY = 'zagens-desktop-browser-destroy-on-close';
const ALLOW_LAN_KEY = 'zagens-desktop-browser-allow-private-lan';

export function readBrowserModePref(): BrowserEmbedMode {
  try {
    const s = localStorage.getItem(MODE_KEY);
    if (s === 'embedded' || s === 'windowed' || s === 'auto') return s;
  } catch {
    /* ignore */
  }
  return 'auto';
}

export function writeBrowserModePref(mode: BrowserEmbedMode): void {
  try {
    localStorage.setItem(MODE_KEY, mode);
  } catch {
    /* ignore */
  }
}

/** Default true: keep clean profile across sessions. */
export function readPersistProfilePref(): boolean {
  try {
    const raw = localStorage.getItem(PERSIST_KEY);
    if (raw == null) return true;
    return raw === '1';
  } catch {
    return true;
  }
}

export function writePersistProfilePref(on: boolean): void {
  try {
    localStorage.setItem(PERSIST_KEY, on ? '1' : '0');
  } catch {
    /* ignore */
  }
}

/**
 * When true, leaving the Browser view destroys the host.
 * When false, hide/keep background (main window close still destroys).
 */
export function readDestroyOnClosePref(): boolean {
  try {
    const raw = localStorage.getItem(DESTROY_ON_CLOSE_KEY);
    if (raw == null) return false;
    return raw === '1';
  } catch {
    return false;
  }
}

export function writeDestroyOnClosePref(on: boolean): void {
  try {
    localStorage.setItem(DESTROY_ON_CLOSE_KEY, on ? '1' : '0');
  } catch {
    /* ignore */
  }
}

export function readAllowPrivateLanPref(): boolean {
  try {
    return localStorage.getItem(ALLOW_LAN_KEY) === '1';
  } catch {
    return false;
  }
}

export function writeAllowPrivateLanPref(on: boolean): void {
  try {
    localStorage.setItem(ALLOW_LAN_KEY, on ? '1' : '0');
  } catch {
    /* ignore */
  }
}

const BROWSER_YOLO_KEY = 'zagens-desktop-browser-yolo';

/** Decoupled from global YOLO — only auto-approves browser write tools. */
export function readBrowserYoloPref(): boolean {
  try {
    return localStorage.getItem(BROWSER_YOLO_KEY) === '1';
  } catch {
    return false;
  }
}

export function writeBrowserYoloPref(on: boolean): void {
  try {
    localStorage.setItem(BROWSER_YOLO_KEY, on ? '1' : '0');
  } catch {
    /* ignore */
  }
}

const POST_EDIT_PREVIEW_HINT_KEY = 'zagens-desktop-post-edit-preview-hint';

/**
 * Soft toast after successful file edits suggesting Browser preview.
 * Default on. Auto-verify (agent-driven) stays off / not implemented.
 */
export function readPostEditPreviewHintPref(): boolean {
  try {
    const raw = localStorage.getItem(POST_EDIT_PREVIEW_HINT_KEY);
    if (raw == null) return true;
    return raw === '1';
  } catch {
    return true;
  }
}

export function writePostEditPreviewHintPref(on: boolean): void {
  try {
    localStorage.setItem(POST_EDIT_PREVIEW_HINT_KEY, on ? '1' : '0');
  } catch {
    /* ignore */
  }
}

const EDIT_TOOLS_FOR_PREVIEW_HINT = new Set([
  'edit_file',
  'write_file',
  'apply_patch',
  'batch_edit',
]);

export function isEditToolForPreviewHint(toolName: string): boolean {
  return EDIT_TOOLS_FOR_PREVIEW_HINT.has(toolName);
}
