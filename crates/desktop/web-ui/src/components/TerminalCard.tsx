import { useEffect, useRef, useState } from 'react';
import '@xterm/xterm/css/xterm.css';
import { useT } from '../i18n';
import { useXterm } from '../lib/terminal/useXterm';
import { xtermThemeForAppDarkMode } from '../lib/terminal/xtermTheme';

export type TerminalToolStatus = 'running' | 'done' | 'error';

/**
 * Initial row count for read-only terminal cards.
 * xterm pre-allocates this many rows; actual scrollback is unlimited.
 * 12 rows ≈ 168 px at font-size 11 — enough to show short command output
 * without excessive blank space for silent commands.
 */
const TERMINAL_CARD_ROWS = 12;

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
 * sequences that were meant for dark terminals and read as "empty" on our light theme.
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

/** Whether there is any visible text after normalizing; strip ANSI so "empty" colored output still counts. */
function hasTerminalText(output: string): boolean {
  const plain = prepareTerminalOutput(output, true);
  return plain.trim().length > 0;
}

/** Only mounted when there is something to show — avoids xterm's fixed min-height slab for silent commands (common with `python …`). */
function TerminalXtermView({ output }: { output: string }) {
  const isDark = useDocumentDarkClass();
  const lightUi = !isDark;

  const writtenLenRef = useRef(0);
  const outputRef = useRef(output);
  outputRef.current = output;
  const lightUiRef = useRef(lightUi);
  lightUiRef.current = lightUi;

  const { termRef, containerRef } = useXterm(
    {
      theme: xtermThemeForAppDarkMode(isDark),
      fontSize: 11,
      rows: TERMINAL_CARD_ROWS,
      disableStdin: true,
    },
    {
      onReady: (term, fit) => {
        writtenLenRef.current = 0;

        const repaint = () => {
          if (!containerRef.current || containerRef.current.clientWidth < 2) return;
          try { fit.fit(); } catch { /* narrow layout */ }
          term.clear();
          const prepared = prepareTerminalOutput(outputRef.current, lightUiRef.current);
          term.write(prepared);
          writtenLenRef.current = prepared.length;
        };

        repaint();
        // t1: catch layouts that resolve after the first synchronous paint
        //     (e.g. chat bubble flex expanding to final width).
        // t2: catch Tauri/Windows WebView second-pass compositing delay.
        const t1 = window.setTimeout(repaint, 50);
        const t2 = window.setTimeout(repaint, 250);

        return () => {
          window.clearTimeout(t1);
          window.clearTimeout(t2);
          writtenLenRef.current = 0;
        };
      },
      onResize: (fitSafe) => {
        // On resize: re-fit then clear+rewrite to avoid wrapping artifacts.
        fitSafe();
        const term = termRef.current;
        if (!term) return;
        term.clear();
        const prepared = prepareTerminalOutput(outputRef.current, lightUiRef.current);
        term.write(prepared);
        writtenLenRef.current = prepared.length;
      },
    },
    [isDark],
  );

  /** Append `tool.progress` chunks without clearing the terminal each frame (F1a). */
  useEffect(() => {
    const term = termRef.current;
    if (!term) return;
    const prepared = prepareTerminalOutput(output, lightUi);
    if (prepared.length < writtenLenRef.current) {
      term.clear();
      term.write(prepared);
      writtenLenRef.current = prepared.length;
      return;
    }
    if (prepared.length > writtenLenRef.current) {
      term.write(prepared.slice(writtenLenRef.current));
      writtenLenRef.current = prepared.length;
    }
  }, [output, lightUi, termRef]);

  return (
    <div ref={containerRef} className="terminal-container w-full min-w-0 px-1 min-h-[8rem]" />
  );
}

function EmptyOutputHint({ status }: { status: TerminalToolStatus }) {
  const { t } = useT();
  const line =
    status === 'running'
      ? t('terminalCard.runningEmpty')
      : status === 'error'
        ? t('terminalCard.errorEmpty')
        : t('terminalCard.silentSuccess');

  return (
    <div className="border-t border-divider bg-canvas px-3 py-2 text-[11px] leading-relaxed text-t-text-muted">
      {line}
    </div>
  );
}

export default function TerminalCard({ output, command, status = 'done' }: Props) {
  const { t } = useT();
  const showXterm = hasTerminalText(output);

  return (
    <div
      className="rounded-lg border border-card-border overflow-hidden my-2"
      role="region"
      aria-label={command ? `Shell: ${command}` : t('terminalCard.shellOutput')}
    >
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
