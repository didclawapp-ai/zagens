import { useEffect, useRef, type CSSProperties, type FocusEvent, type MouseEvent } from 'react';
import type { FitAddon } from '@xterm/addon-fit';
import { SearchAddon } from '@xterm/addon-search';
import type { ITheme } from '@xterm/xterm';
import '@xterm/xterm/css/xterm.css';
import { WebLinksAddon } from '@xterm/addon-web-links';
import { useT } from '../../i18n';
import {
  focusIntegratedTerminal,
  primeCursor,
  writeToDisplay,
} from '../../lib/terminal/cursorVisibility';
import { useXterm } from '../../lib/terminal/useXterm';
import { resizeTerminal, writeTerminal } from '../../lib/terminal/ptyApi';

export interface TerminalViewActions {
  clear: () => void;
  findNext: (query: string) => boolean;
  findPrevious: (query: string) => boolean;
}

interface Props {
  sessionId: string;
  /** Replay buffer accumulated by TerminalPanel (includes offline chunks). */
  outputBuffer: string;
  theme: ITheme;
  fontSize: number;
  copyOnSelect: boolean;
  /** Register a live writer so the panel can push chunks while this view is mounted. */
  registerWriter: (sessionId: string, write: (chunk: string) => void) => () => void;
  /** Register clear/search actions for the panel chrome. */
  registerActions: (sessionId: string, actions: TerminalViewActions) => () => void;
  active: boolean;
}

const INTEGRATED_SCROLLBACK = 5000;

export default function InteractiveTerminalView({
  sessionId,
  outputBuffer,
  theme,
  fontSize,
  copyOnSelect,
  registerWriter,
  registerActions,
  active,
}: Props) {
  const { t } = useT();
  const tRef = useRef(t);
  tRef.current = t;

  const activeRef = useRef(active);
  activeRef.current = active;
  const bufferRef = useRef(outputBuffer);
  bufferRef.current = outputBuffer;
  const sessionIdRef = useRef(sessionId);
  sessionIdRef.current = sessionId;
  const registerWriterRef = useRef(registerWriter);
  registerWriterRef.current = registerWriter;
  const registerActionsRef = useRef(registerActions);
  registerActionsRef.current = registerActions;
  const copyOnSelectRef = useRef(copyOnSelect);
  copyOnSelectRef.current = copyOnSelect;
  const fitInstanceRef = useRef<FitAddon | null>(null);

  const syncPtySize = () => {
    if (!activeRef.current) return;
    const fit = fitInstanceRef.current;
    if (!fit) return;
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

  const { containerRef, termRef } = useXterm(
    {
      theme,
      fontSize,
      scrollback: INTEGRATED_SCROLLBACK,
      cursorBlink: true,
      cursorStyle: 'block',
      cursorInactiveStyle: 'outline',
      cursorWidth: 2,
    },
    {
      onReady: (term, fit) => {
        fitInstanceRef.current = fit;
        term.options.fontFamily =
          "JetBrains Mono, ui-monospace, Cascadia Code, Consolas, monospace";

        const webLinks = new WebLinksAddon();
        term.loadAddon(webLinks);
        const search = new SearchAddon();
        term.loadAddon(search);

        // Swallow lone DECTCEM hide so ConPTY cannot stick "cursor off" in the parser.
        const hideCursorHandler = term.parser.registerCsiHandler(
          { prefix: '?', final: 'l' },
          (params) => {
            const flat = params.flatMap((p) => (Array.isArray(p) ? p : [p]));
            return flat.length === 1 && flat[0] === 25;
          },
        );

        if (bufferRef.current) {
          writeToDisplay(term, bufferRef.current);
        }
        primeCursor(term);

        // xterm 5.5 skips unmodified Space (`keyCode` 32) in evaluateKeyboardEvent;
        // WebView2 may also omit keypress — inject the character ourselves.
        const container = containerRef.current;
        const onSpaceKey = (e: KeyboardEvent) => {
          if (
            e.key === ' ' &&
            !e.ctrlKey &&
            !e.altKey &&
            !e.metaKey &&
            !e.shiftKey
          ) {
            e.preventDefault();
            e.stopPropagation();
            if (e.type === 'keydown') {
              void writeTerminal(sessionIdRef.current, ' ').catch(() => {});
            }
          }
        };
        if (container) {
          container.addEventListener('keydown', onSpaceKey, true);
          container.addEventListener('keypress', onSpaceKey, true);
        }

        const dataSub = term.onData((data) => {
          void writeTerminal(sessionIdRef.current, data).catch(() => {
            writeToDisplay(
              term,
              `\r\n\x1b[31m${tRef.current('terminalInteractive.writeFailed')}\x1b[0m\r\n`,
            );
          });
        });

        const selectionSub = term.onSelectionChange(() => {
          if (!copyOnSelectRef.current) return;
          const sel = term.getSelection();
          if (!sel) return;
          void navigator.clipboard.writeText(sel).catch(() => {});
        });

        const unregisterWriter = registerWriterRef.current(sessionIdRef.current, (chunk) => {
          writeToDisplay(term, chunk);
        });

        const unregisterActions = registerActionsRef.current(sessionIdRef.current, {
          clear: () => {
            term.clear();
            term.scrollToBottom();
          },
          findNext: (query) => {
            if (!query) return false;
            return search.findNext(query);
          },
          findPrevious: (query) => {
            if (!query) return false;
            return search.findPrevious(query);
          },
        });

        const timer = window.setTimeout(syncPtySize, 100);

        return () => {
          window.clearTimeout(timer);
          if (container) {
            container.removeEventListener('keydown', onSpaceKey, true);
            container.removeEventListener('keypress', onSpaceKey, true);
          }
          dataSub.dispose();
          selectionSub.dispose();
          unregisterWriter();
          unregisterActions();
          hideCursorHandler.dispose();
          search.dispose();
          webLinks.dispose();
          fitInstanceRef.current = null;
        };
      },
      onResize: (fitSafe) => {
        fitSafe();
        if (!activeRef.current) return;
        const fit = fitInstanceRef.current;
        if (!fit) return;
        try {
          const dims = fit.proposeDimensions();
          if (dims?.cols && dims?.rows) {
            void resizeTerminal(sessionIdRef.current, dims.cols, dims.rows).catch(() => {});
          }
        } catch {
          /* ignore */
        }
      },
    },
    [sessionId],
  );

  useEffect(() => {
    const term = termRef.current;
    if (!term) return;
    term.options.theme = theme;
    term.options.fontSize = fontSize;
    syncPtySize();
  }, [theme, fontSize, termRef]);

  useEffect(() => {
    if (!active) return;
    const timer = window.setTimeout(() => {
      const term = termRef.current;
      const container = containerRef.current;
      if (!term || !container || container.clientWidth < 2) return;
      syncPtySize();
      focusIntegratedTerminal(term);
    }, 50);
    return () => window.clearTimeout(timer);
  }, [active, sessionId, termRef, containerRef]);

  const focusTerminal = (e?: MouseEvent | FocusEvent) => {
    // Do not preventDefault — that would break drag-select / copy-on-select.
    e?.stopPropagation?.();
    const term = termRef.current;
    if (term) focusIntegratedTerminal(term);
  };

  return (
    <div
      ref={containerRef}
      className="terminal-panel-xterm h-full w-full min-h-0 min-w-0 px-1 py-1"
      aria-label={t('terminal.tab')}
      style={
        {
          '--zagens-term-cursor': theme.cursor ?? '#4ade80',
          '--zagens-term-bg': theme.background ?? '#121212',
        } as CSSProperties
      }
      onMouseDownCapture={focusTerminal}
      onFocus={focusTerminal}
      tabIndex={0}
    />
  );
}
