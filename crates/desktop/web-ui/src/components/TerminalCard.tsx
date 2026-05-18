import { useEffect, useRef, useState } from 'react';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import { xtermThemeForAppDarkMode } from '../lib/terminal/xtermTheme';

export type TerminalToolStatus = 'running' | 'done' | 'error';

interface Props {
  /** Output text — re-renders the terminal when it changes */
  output: string;
  /** Shell command that produced this output (shown in header) */
  command?: string;
  /** Tool lifecycle — used when there is no stdout/stderr yet or ever */
  status?: TerminalToolStatus;
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
    // CSI … final byte (ECMA-48): param bytes, optional intermediates, final 0x40–0x7e
    s = s.replace(/\u001b\[[\x30-\x3f]*[\x20-\x2f]*[\x40-\x7e]/g, '');
    s = s.replace(/\u001b[ -/][@-~]/g, '');
  }
  return s;
}

/** Whether there is any visible text after normalizing; strip ANSI so “empty” colored output still counts. */
function hasTerminalText(output: string): boolean {
  const plain = prepareTerminalOutput(output, true);
  return plain.trim().length > 0;
}

/** Only mounted when there is something to show — avoids xterm’s fixed min-height slab for silent commands (common with `python …`). */
function TerminalXtermView({ output }: { output: string }) {
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
      if (container.clientWidth < 2 || container.clientHeight < 2) return;
      try {
        fit.fit();
      } catch {
        /* narrow flex layouts may throw until width stabilizes */
      }
      term.clear();
      term.write(prepareTerminalOutput(outputRef.current, lightUi));
    };

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
    const container = containerRef.current;
    if (!term || !container || container.clientWidth < 2 || container.clientHeight < 2) return;
    try {
      fitRef.current?.fit();
    } catch {
      /* ignore */
    }
    term.clear();
    term.write(prepareTerminalOutput(output, !isDark));
  }, [output, isDark]);

  return (
    <div ref={containerRef} className="terminal-container w-full min-w-0 px-1 min-h-[8rem]" />
  );
}

function EmptyOutputHint({ status }: { status: TerminalToolStatus }) {
  const line =
    status === 'running'
      ? '正在执行，尚未收到终端输出…（不少 Python 脚本成功时也不打印任何内容）'
      : status === 'error'
        ? '命令失败，且无 stderr/stdout 文本。'
        : '命令未向终端打印任何内容（常见于脚本静默成功，仅由退出码表示结果）。';

  return (
    <div className="border-t border-divider bg-canvas px-3 py-2 text-[11px] leading-relaxed text-t-text-muted">
      {line}
    </div>
  );
}

export default function TerminalCard({ output, command, status = 'done' }: Props) {
  const showXterm = hasTerminalText(output);

  return (
    <div className="rounded-lg border border-card-border overflow-hidden my-2">
      <div className="flex items-center gap-2 px-3 py-1.5 bg-canvas-alt border-b border-divider">
        <span className="w-2.5 h-2.5 rounded-full bg-t-error/70" />
        <span className="w-2.5 h-2.5 rounded-full bg-amber/70" />
        <span className="w-2.5 h-2.5 rounded-full bg-success/70" />
        <span className="ml-2 text-[10px] text-t-text-muted font-mono truncate flex-1">
          {command || 'shell'}
        </span>
      </div>
      {showXterm ? <TerminalXtermView output={output} /> : <EmptyOutputHint status={status} />}
    </div>
  );
}
