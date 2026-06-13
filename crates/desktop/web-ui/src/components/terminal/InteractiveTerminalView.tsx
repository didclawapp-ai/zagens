import { useEffect, useRef } from 'react';
import '@xterm/xterm/css/xterm.css';
import { useT } from '../../i18n';
import { useXterm } from '../../lib/terminal/useXterm';
import { subscribeCurrentWebviewEvent } from '../../lib/tauriListen';
import { integratedTerminalTheme } from '../../lib/terminal/xtermTheme';
import { resizeTerminal, writeTerminal, type TerminalDataEvent, type TerminalExitEvent } from '../../lib/terminal/ptyApi';

interface Props {
  sessionId: string;
  /** Replay + append streamed output */
  outputBuffer: string;
  onOutput: (sessionId: string, chunk: string) => void;
  onExit: (sessionId: string, code: number | null) => void;
  active: boolean;
}

export default function InteractiveTerminalView({
  sessionId,
  outputBuffer,
  onOutput,
  onExit,
  active,
}: Props) {
  const { t } = useT();
  const tRef = useRef(t);
  tRef.current = t;

  const activeRef = useRef(active);
  activeRef.current = active;
  const bufferRef = useRef(outputBuffer);
  bufferRef.current = outputBuffer;
  const onOutputRef = useRef(onOutput);
  onOutputRef.current = onOutput;
  const onExitRef = useRef(onExit);
  onExitRef.current = onExit;
  const sessionIdRef = useRef(sessionId);
  sessionIdRef.current = sessionId;

  const { containerRef, termRef, fitRef } = useXterm(
    { theme: integratedTerminalTheme, fontSize: 12, cursorBlink: true },
    {
      onReady: (term, fit, fitSafe) => {
        if (bufferRef.current) {
          term.write(bufferRef.current);
        }

        const dataSub = term.onData((data) => {
          void writeTerminal(sessionIdRef.current, data).catch(() => {
            term.write(`\r\n\x1b[31m${tRef.current('terminalInteractive.writeFailed')}\x1b[0m\r\n`);
          });
        });

        const unlistenData = subscribeCurrentWebviewEvent<TerminalDataEvent>('terminal-data', (payload) => {
          if (payload.id !== sessionIdRef.current) return;
          onOutputRef.current(sessionIdRef.current, payload.data);
          term.write(payload.data);
        });

        const unlistenExit = subscribeCurrentWebviewEvent<TerminalExitEvent>('terminal-exit', (payload) => {
          if (payload.id !== sessionIdRef.current) return;
          onExitRef.current(sessionIdRef.current, payload.code);
          const code = payload.code;
          const line =
            code != null && code !== 0
              ? `\r\n\x1b[90m${tRef.current('terminalInteractive.processExited', { code: String(code) })}\x1b[0m\r\n`
              : `\r\n\x1b[90m${tRef.current('terminalInteractive.processEnded')}\x1b[0m\r\n`;
          term.write(line);
        });

        // Sync PTY size after initial layout stabilises.
        const syncSize = () => {
          if (!activeRef.current) return;
          try {
            fit.fit();
            const dims = fit.proposeDimensions();
            if (dims?.cols && dims?.rows) {
              void resizeTerminal(sessionIdRef.current, dims.cols, dims.rows).catch(() => {});
            }
          } catch {
            /* ignore */
          }
        };
        // Piggyback on the hook's own fitSafe for resize observer; run syncSize
        // once after mount to push initial cols/rows to the PTY backend.
        const t = window.setTimeout(syncSize, 100);

        return () => {
          window.clearTimeout(t);
          dataSub.dispose();
          unlistenData();
          unlistenExit();
        };
      },
    },
    [sessionId],
  );

  // Re-fit and focus when the panel tab becomes active.
  useEffect(() => {
    if (!active) return;
    const timer = window.setTimeout(() => {
      const fit = fitRef.current;
      const term = termRef.current;
      const container = containerRef.current;
      if (!fit || !term || !container || container.clientWidth < 2) return;
      try {
        fit.fit();
        const dims = fit.proposeDimensions();
        if (dims?.cols && dims?.rows) {
          void resizeTerminal(sessionId, dims.cols, dims.rows).catch(() => {});
        }
        term.focus();
      } catch {
        /* ignore */
      }
    }, 50);
    return () => window.clearTimeout(timer);
  }, [active, sessionId, fitRef, termRef, containerRef]);

  return (
    <div
      ref={containerRef}
      className="terminal-panel-xterm h-full w-full min-h-0 min-w-0 px-1 py-1"
      aria-label={t('terminal.tab')}
    />
  );
}
