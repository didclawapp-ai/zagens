import { useEffect, useRef } from 'react';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';

interface Props {
  /** Output text — re-renders the terminal when it changes */
  output: string;
  /** Shell command that produced this output (shown in header) */
  command?: string;
}

export default function TerminalCard({ output, command }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);

  useEffect(() => {
    if (!containerRef.current) return;

    const term = new Terminal({
      cursorBlink: false,
      disableStdin: true,
      fontSize: 11,
      fontFamily: "'Cascadia Code', 'Consolas', 'Fira Code', monospace",
      theme: {
        background: '#0a0a0a',
        foreground: '#e0e0e0',
        cursor: '#6366f1',
        selectionBackground: '#374151',
      },
      rows: 12,
    });

    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(containerRef.current);

    // Fit after a short delay so the container has dimensions
    setTimeout(() => {
      try { fit.fit(); } catch { /* ignore */ }
    }, 50);

    termRef.current = term;
    fitRef.current = fit;

    return () => {
      term.dispose();
    };
  }, []);

  useEffect(() => {
    const term = termRef.current;
    if (!term) return;
    // Clear and rewrite output
    term.clear();
    term.write(output.replace(/\r\n/g, '\n').replace(/\r/g, '\n'));
  }, [output]);

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
      <div ref={containerRef} className="terminal-container" />
    </div>
  );
}
