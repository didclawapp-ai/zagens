import type { ITheme } from '@xterm/xterm';

/** Read a CSS custom property from the root element. */
function cssVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

/**
 * ANSI 16-color palette for dark-background terminals.
 * Shared by both the integrated terminal and the dark TerminalCard.
 */
const ANSI_DARK = {
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
} as const;

/**
 * ANSI 16-color palette for light-background terminals.
 * Used by TerminalCard in light app mode.
 */
const ANSI_LIGHT = {
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
} as const;

/**
 * Read-only tool output in chat (TerminalCard).
 * Background, foreground, and cursor follow the app's CSS design tokens so
 * the card blends into the chat canvas in both light and dark modes.
 */
export function xtermThemeForAppDarkMode(isDark: boolean): ITheme {
  if (isDark) {
    return {
      background: cssVar('--color-canvas-alt'),
      foreground: cssVar('--color-text'),
      cursor: cssVar('--color-accent'),
      selectionBackground: '#3f3f4680',
      ...ANSI_DARK,
    };
  }
  return {
    background: cssVar('--color-canvas'),
    foreground: cssVar('--color-text'),
    cursor: cssVar('--color-accent'),
    selectionBackground: cssVar('--color-hover-strong'),
    ...ANSI_LIGHT,
  };
}

/**
 * Integrated terminal (right-panel workspace tab).
 * Always dark — matches the terminal panel's own dark chrome (`bg-[#121212]`).
 * Foreground and cursor are fixed neutral values that work on this background.
 */
export const integratedTerminalTheme: ITheme = {
  background: '#121212',
  foreground: '#e4e4e7',
  /** Block/bar fill on empty cells — must contrast with background and foreground. */
  cursor: '#4ade80',
  /** Foreground on block cursor when over a character. */
  cursorAccent: '#052e16',
  selectionBackground: '#3f3f4680',
  ...ANSI_DARK,
};
