import { useEffect, useRef } from 'react';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
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

function containerHasLayout(el: HTMLElement | null): boolean {
  return el != null && el.clientWidth >= 2 && el.clientHeight >= 2;
}

export default function InteractiveTerminalView({
  sessionId,
  outputBuffer,
  onOutput,
  onExit,
  active,
}: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const disposedRef = useRef(false);
  const activeRef = useRef(active);
  activeRef.current = active;
  const bufferRef = useRef(outputBuffer);
  bufferRef.current = outputBuffer;
  const onOutputRef = useRef(onOutput);
  onOutputRef.current = onOutput;
  const onExitRef = useRef(onExit);
  onExitRef.current = onExit;

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    disposedRef.current = false;

    const term = new Terminal({
      cursorBlink: true,
      convertEol: true,
      fontSize: 12,
      fontFamily: 'var(--font-mono)',
      theme: integratedTerminalTheme,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(container);

    const fitSafe = () => {
      if (disposedRef.current || !activeRef.current) return;
      if (!containerHasLayout(container)) return;
      try {
        fit.fit();
        const dims = fit.proposeDimensions();
        if (dims?.cols && dims?.rows) {
          void resizeTerminal(sessionId, dims.cols, dims.rows).catch(() => {});
        }
      } catch {
        /* layout not ready or terminal tearing down */
      }
    };

    if (bufferRef.current) {
      term.write(bufferRef.current);
    }

    const dataSub = term.onData((data) => {
      if (disposedRef.current) return;
      void writeTerminal(sessionId, data).catch(() => {
        if (!disposedRef.current) {
          term.write('\r\n\x1b[31m[终端写入失败]\x1b[0m\r\n');
        }
      });
    });

    const unlistenData = subscribeCurrentWebviewEvent<TerminalDataEvent>('terminal-data', (payload) => {
      if (disposedRef.current || payload.id !== sessionId) return;
      onOutputRef.current(sessionId, payload.data);
      term.write(payload.data);
    });

    const unlistenExit = subscribeCurrentWebviewEvent<TerminalExitEvent>('terminal-exit', (payload) => {
      if (disposedRef.current || payload.id !== sessionId) return;
      onExitRef.current(sessionId, payload.code);
      const code = payload.code;
      const line =
        code != null && code !== 0
          ? `\r\n\x1b[90m[进程已退出，代码 ${code}]\x1b[0m\r\n`
          : '\r\n\x1b[90m[进程已结束]\x1b[0m\r\n';
      term.write(line);
    });

    let roRaf = 0;
    const ro = new ResizeObserver(() => {
      if (!activeRef.current) return;
      cancelAnimationFrame(roRaf);
      roRaf = window.requestAnimationFrame(fitSafe);
    });
    ro.observe(container);

    const initialFit = window.setTimeout(fitSafe, 0);
    const delayedFit = window.setTimeout(fitSafe, 80);

    termRef.current = term;
    fitRef.current = fit;

    return () => {
      disposedRef.current = true;
      window.clearTimeout(initialFit);
      window.clearTimeout(delayedFit);
      dataSub.dispose();
      unlistenData();
      unlistenExit();
      cancelAnimationFrame(roRaf);
      ro.disconnect();
      term.dispose();
      termRef.current = null;
      fitRef.current = null;
    };
  }, [sessionId]);

  useEffect(() => {
    if (!active) return;
    const t = window.setTimeout(() => {
      if (disposedRef.current) return;
      const container = containerRef.current;
      if (!containerHasLayout(container)) return;
      try {
        fitRef.current?.fit();
        const dims = fitRef.current?.proposeDimensions();
        if (dims?.cols && dims?.rows) {
          void resizeTerminal(sessionId, dims.cols, dims.rows).catch(() => {});
        }
        termRef.current?.focus();
      } catch {
        /* ignore */
      }
    }, 50);
    return () => window.clearTimeout(t);
  }, [active, sessionId]);

  return (
    <div
      ref={containerRef}
      className="terminal-panel-xterm h-full w-full min-h-0 min-w-0 px-1 py-1"
      role="tabpanel"
      aria-label="Terminal"
    />
  );
}
