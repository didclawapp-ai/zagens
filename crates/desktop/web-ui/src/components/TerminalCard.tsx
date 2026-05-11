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

/**
 * Normalize captured shell output for read-only xterm display.
 * In light UI, strip SGR/OSC — PowerShell and Windows consoles often emit pale / true-color
 * sequences that were meant for dark terminals and read as “empty” on our light theme.
 */
function prepareTerminalOutput(text: string, lightUi: boolean): string {
  let s = text
    .replace(/\u0000/g, '')
    .replace(/\r\n/g, '\n')
    .replace(/\r/g, '\n');
  if (lightUi) {
    // OSC (hyperlinks, VS Code / pwsh semantic prompts, etc.)
    s = s.replace(/\u001b\][^\u0007\u001b]*(?:\u0007|\u001b\\)/g, '');
    // CSI (SGR colors, cursor motion — dropping is fine for static logs)
    // CSI … final byte (ECMA-48): param bytes, optional intermediates, final 0x40–0x7e
    s = s.replace(/\u001b\[[\x30-\x3f]*[\x20-\x2f]*[\x40-\x7e]/g, '');
    // Other two-char ESC sequences
    s = s.replace(/\u001b[ -/][@-~]/g, '');
  }
  return s;
}

export default function TerminalCard({ output, command }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const outputRef = useRef(output);
  outputRef.current = output;
  const isDark = useDocumentDarkClass();

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const term = new Terminal({
      cursorBlink: false,
      disableStdin: true,
      convertEol: true,
      fontSize: 11,
      fontFamily: "'Cascadia Code', 'Consolas', 'Fira Code', monospace",
      theme: xtermThemeForAppDarkMode(isDark),
      rows: 12,
    });

    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(container);

    const lightUi = !isDark;
    const repaint = () => {
      try {
        fit.fit();
      } catch {
        /* narrow flex layouts may throw until width stabilizes */
      }
      term.clear();
      term.write(prepareTerminalOutput(outputRef.current, lightUi));
    };

    // Initial paint + deferred fits: first open often sees width 0 in flex; fit() then fixes cols.
    repaint();
    const t1 = window.setTimeout(repaint, 50);
    const t2 = window.setTimeout(repaint, 250);

    let roRaf = 0;
    const ro = new ResizeObserver(() => {
      cancelAnimationFrame(roRaf);
      roRaf = window.requestAnimationFrame(repaint);
    });
    ro.observe(container);

    termRef.current = term;
    fitRef.current = fit;

    return () => {
      window.clearTimeout(t1);
      window.clearTimeout(t2);
      cancelAnimationFrame(roRaf);
      ro.disconnect();
      term.dispose();
      termRef.current = null;
      fitRef.current = null;
    };
  }, [isDark]);

  useEffect(() => {
    const term = termRef.current;
    if (!term) return;
    try {
      fitRef.current?.fit();
    } catch {
      /* ignore */
    }
    term.clear();
    term.write(prepareTerminalOutput(output, !isDark));
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
      <div ref={containerRef} className="terminal-container w-full min-w-0 px-1 min-h-[8rem]" />
    </div>
  );
}
