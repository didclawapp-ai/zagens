import { useEffect, useRef } from 'react';
import { Terminal, type ITheme } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';

export interface UseXtermOptions {
  theme: ITheme;
  fontSize?: number;
  rows?: number;
  cursorBlink?: boolean;
  cursorStyle?: 'block' | 'underline' | 'bar';
  cursorInactiveStyle?: 'outline' | 'block' | 'bar' | 'underline' | 'none';
  cursorWidth?: number;
  disableStdin?: boolean;
}

export interface UseXtermHandle {
  termRef: React.RefObject<Terminal | null>;
  fitRef: React.RefObject<FitAddon | null>;
  containerRef: React.RefObject<HTMLDivElement>;
}

function hasLayout(el: HTMLElement | null): boolean {
  return el != null && el.clientWidth >= 2 && el.clientHeight >= 2;
}

/**
 * Shared xterm.js lifecycle hook.
 *
 * Mounts a Terminal + FitAddon into `containerRef`, observes resizes,
 * and tears everything down on unmount. The caller writes data and attaches
 * event listeners via `onReady` and `onDispose` callbacks.
 *
 * Two deferred fit calls (0 ms and `fitDelayMs`) handle layouts that aren't
 * ready synchronously (e.g. flex containers resolving after paint).
 *
 * @param onResize  Optional: override the default resize handler. When provided,
 *                  the hook calls this instead of `fitSafe` in the ResizeObserver.
 *                  Receives `fitSafe` so the overrider can compose it.
 */
export function useXterm(
  options: UseXtermOptions,
  callbacks: {
    onReady: (term: Terminal, fit: FitAddon, fitSafe: () => void) => (() => void) | void;
    onResize?: (fitSafe: () => void) => void;
  },
  deps: React.DependencyList = [],
): UseXtermHandle {
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const disposedRef = useRef(false);

  const callbacksRef = useRef(callbacks);
  callbacksRef.current = callbacks;

  const optionsRef = useRef(options);
  optionsRef.current = options;

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    disposedRef.current = false;

    const {
      theme,
      fontSize = 12,
      rows,
      cursorBlink = false,
      cursorStyle = 'bar',
      cursorInactiveStyle = 'block',
      cursorWidth = 2,
      disableStdin = false,
    } = optionsRef.current;

    const term = new Terminal({
      cursorBlink,
      cursorStyle,
      cursorInactiveStyle,
      cursorWidth,
      disableStdin,
      convertEol: true,
      fontSize,
      fontFamily: 'var(--font-mono)',
      theme,
      ...(rows != null ? { rows } : {}),
    });

    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(container);

    const fitSafe = () => {
      if (disposedRef.current || !hasLayout(container)) return;
      try {
        fit.fit();
      } catch {
        /* narrow flex layout may throw until width stabilizes */
      }
    };

    const cleanup = callbacksRef.current.onReady(term, fit, fitSafe);

    let roRaf = 0;
    const resizeHandler = callbacksRef.current.onResize ?? fitSafe;
    const ro = new ResizeObserver(() => {
      cancelAnimationFrame(roRaf);
      roRaf = window.requestAnimationFrame(() => resizeHandler(fitSafe));
    });
    ro.observe(container);

    // Two deferred fits cover two failure modes:
    // t0 (0 ms): xterm.open() reports cols=0 synchronously in some flex layouts;
    //            a microtask-delayed fit picks up the first measured width.
    // t1 (80 ms): Tauri WebView on Windows may defer compositing; a second fit
    //             catches the stabilised size after the first paint completes.
    const t0 = window.setTimeout(fitSafe, 0);
    const t1 = window.setTimeout(fitSafe, 80);

    termRef.current = term;
    fitRef.current = fit;

    return () => {
      disposedRef.current = true;
      cleanup?.();
      window.clearTimeout(t0);
      window.clearTimeout(t1);
      cancelAnimationFrame(roRaf);
      ro.disconnect();
      term.dispose();
      termRef.current = null;
      fitRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);

  return { termRef, fitRef, containerRef };
}
