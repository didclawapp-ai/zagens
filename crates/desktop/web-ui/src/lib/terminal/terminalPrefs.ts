/** Helpers for integrated-terminal preferences and shell CD commands. */

import type { TerminalShellKind } from './ptyApi';

export const FONT_SIZE_PREF_KEY = 'zagens-desktop-terminal-font-size';
export const COPY_ON_SELECT_PREF_KEY = 'zagens-desktop-terminal-copy-on-select';

export const TERMINAL_FONT_SIZES = [11, 12, 13, 14] as const;
export type TerminalFontSize = (typeof TERMINAL_FONT_SIZES)[number];

export function readFontSizePref(): TerminalFontSize {
  try {
    const n = parseInt(localStorage.getItem(FONT_SIZE_PREF_KEY) ?? '', 10);
    if ((TERMINAL_FONT_SIZES as readonly number[]).includes(n)) {
      return n as TerminalFontSize;
    }
  } catch {
    /* ignore */
  }
  return 12;
}

export function writeFontSizePref(size: TerminalFontSize): void {
  try {
    localStorage.setItem(FONT_SIZE_PREF_KEY, String(size));
  } catch {
    /* ignore */
  }
}

export function readCopyOnSelectPref(): boolean {
  try {
    const raw = localStorage.getItem(COPY_ON_SELECT_PREF_KEY);
    if (raw == null) return true;
    return raw === '1';
  } catch {
    return true;
  }
}

export function writeCopyOnSelectPref(on: boolean): void {
  try {
    localStorage.setItem(COPY_ON_SELECT_PREF_KEY, on ? '1' : '0');
  } catch {
    /* ignore */
  }
}

/** Build a newline-terminated `cd` for the active shell kind. */
export function buildCdCommand(
  shell: TerminalShellKind,
  absPath: string,
  isWindows: boolean,
): string {
  const path = absPath.trim();
  if (!path) return '';
  if (isWindows) {
    if (shell === 'cmd') {
      const escaped = path.replace(/"/g, '');
      return `cd /d "${escaped}"\r`;
    }
    // PowerShell (default / pwsh / powershell)
    const escaped = path.replace(/'/g, "''");
    return `Set-Location -LiteralPath '${escaped}'\r`;
  }
  // POSIX — single-quote with '\'' escaping
  const escaped = `'${path.replace(/'/g, `'\\''`)}'`;
  return `cd ${escaped}\n`;
}
