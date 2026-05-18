import type { ITheme } from '@xterm/xterm';

/** Claude / VS Code–style integrated terminal (dark). */
export const integratedTerminalTheme: ITheme = {
  background: '#121212',
  foreground: '#e4e4e7',
  cursor: '#e4e4e7',
  selectionBackground: '#3f3f4680',
  black: '#a1a1aa',
  red: '#f87171',
  green: '#4ade80',
  yellow: '#facc15',
  blue: '#60a5fa',
  magenta: '#c084fc',
  cyan: '#22d3ee',
  white: '#f4f4f5',
  brightBlack: '#71717a',
  brightRed: '#fca5a5',
  brightGreen: '#86efac',
  brightYellow: '#fde047',
  brightBlue: '#93c5fd',
  brightMagenta: '#d8b4fe',
  brightCyan: '#67e8f9',
  brightWhite: '#ffffff',
};

/** Keep in sync with `globals.css` for read-only tool cards in chat. */
export function xtermThemeForAppDarkMode(isDark: boolean): ITheme {
  if (isDark) {
    return {
      background: '#161d26',
      foreground: '#e4e4e7',
      cursor: '#818cf8',
      selectionBackground: '#3f3f46',
      black: '#a1a1aa',
      red: '#f87171',
      green: '#4ade80',
      yellow: '#facc15',
      blue: '#60a5fa',
      magenta: '#c084fc',
      cyan: '#22d3ee',
      white: '#f4f4f5',
      brightBlack: '#71717a',
      brightRed: '#fca5a5',
      brightGreen: '#86efac',
      brightYellow: '#fde047',
      brightBlue: '#93c5fd',
      brightMagenta: '#d8b4fe',
      brightCyan: '#67e8f9',
      brightWhite: '#ffffff',
    };
  }
  return {
    background: '#fafbfc',
    foreground: '#1a1d24',
    cursor: '#2563eb',
    selectionBackground: '#e4e6eb',
    black: '#4b5563',
    red: '#b91c1c',
    green: '#15803d',
    yellow: '#a16207',
    blue: '#1d4ed8',
    magenta: '#7c3aed',
    cyan: '#0e7490',
    white: '#52525b',
    brightBlack: '#6b7280',
    brightRed: '#dc2626',
    brightGreen: '#16a34a',
    brightYellow: '#ca8a04',
    brightBlue: '#2563eb',
    brightMagenta: '#9333ea',
    brightCyan: '#0891b2',
    brightWhite: '#111827',
  };
}
