import { useEffect, useRef, useState } from 'react';
import { Terminal, type ITheme } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';

interface Props {
  /** Output text — re-renders the terminal when it changes */
  output: string;
  /** Shell command that produced this output (shown in header) */
  command?: string;
}

/** Keep in sync with `globals.css` so the panel does not look like a solid black slab in light UI. */
function xtermThemeForAppDarkMode(isDark: boolean): ITheme {
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

function useDocumentDarkClass(): boolean {
  const [dark, setDark] = useState(
    () => typeof document !== 'undefined' && document.documentElement.classList.contains('dark'),
  );

  useEffect(() => {
    const el = document.documentElement;
    const sync = () => setDark(el.classList.contains('dark'));
    sync();
    const obs = new MutationObserver(sync);
    obs.observe(el, { attributes: true, attributeFilter: ['class'] });
    return () => obs.disconnect();
  }, []);

  return dark;
}

export default function TerminalCard({ output, command }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const isDark = useDocumentDarkClass();

  useEffect(() => {
    if (!containerRef.current) return;

    const term = new Terminal({
      cursorBlink: false,
      disableStdin: true,
      fontSize: 11,
      fontFamily: "'Cascadia Code', 'Consolas', 'Fira Code', monospace",
      theme: xtermThemeForAppDarkMode(isDark),
      rows: 12,
    });

    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(containerRef.current);

    // Fit after a short delay so the container has dimensions
    setTimeout(() => {
      try {
        fit.fit();
      } catch {
        /* ignore */
      }
    }, 50);

    termRef.current = term;
    fitRef.current = fit;

    return () => {
      term.dispose();
      termRef.current = null;
      fitRef.current = null;
    };
  }, [isDark]);

  useEffect(() => {
    const term = termRef.current;
    if (!term) return;
    // Clear and rewrite output
    term.clear();
    term.write(output.replace(/\r\n/g, '\n').replace(/\r/g, '\n'));
  }, [output, isDark]);

  return (
    <div className="rounded-lg border border-card-border overflow-hidden my-2">
      {/* Terminal chrome bar */}
      <div className="flex items-center gap-2 px-3 py-1.5 bg-canvas-alt border-b border-divider">
        <span className="w-2.5 h-2.5 rounded-full bg-t-error/70" />
        <span className="w-2.5 h-2.5 rounded-full bg-amber/70" />
        <span className="w-2.5 h-2.5 rounded-full bg-success/70" />
        <span className="ml-2 text-[10px] text-t-text-muted font-mono truncate flex-1">
          {command || 'shell'}
        </span>
      </div>
      {/* xterm container */}
      <div ref={containerRef} className="terminal-container px-1 min-h-[8rem]" />
    </div>
  );
}
