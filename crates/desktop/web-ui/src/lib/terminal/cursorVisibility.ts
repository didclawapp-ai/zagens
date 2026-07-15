import type { Terminal } from '@xterm/xterm';

/** DECSET Cursor Enable Mode — hide (`CSI ? 25 l`). */
const CURSOR_HIDE_RE = /\u001b\[\?25l/gi;

type XtermCoreService = {
  isCursorInitialized?: boolean;
  isCursorHidden?: boolean;
};

type XtermCore = {
  coreService?: XtermCoreService;
};

/**
 * Windows ConPTY / PowerShell often emit DECTCEM hide while drawing the prompt.
 * Strip hide sequences from display-bound data (local only; never re-sent to PTY).
 */
export function sanitizeCursorVisibility(data: string): string {
  return data.replace(CURSOR_HIDE_RE, '');
}

/** Write PTY/output text into the xterm display (hide sequences stripped). */
export function writeToDisplay(term: Terminal, data: string): void {
  if (!data) return;
  const cleaned = sanitizeCursorVisibility(data);
  if (cleaned) term.write(cleaned);
}

/**
 * Dom renderer only emits `.xterm-cursor` when `isCursorInitialized` is true,
 * and skips it when `isCursorHidden`. Focus alone does not always clear hide
 * after early ConPTY bytes — set both so outline/block can paint.
 */
export function primeCursor(term: Terminal): void {
  const core = (term as unknown as { _core?: XtermCore })._core;
  if (core?.coreService) {
    core.coreService.isCursorInitialized = true;
    core.coreService.isCursorHidden = false;
  }
  try {
    term.refresh(0, Math.max(0, term.rows - 1));
  } catch {
    /* ignore */
  }
}

/** Focus the helper textarea (solid blinking caret) and ensure the caret can paint. */
export function focusIntegratedTerminal(term: Terminal): void {
  try {
    term.focus();
  } catch {
    /* ignore */
  }
  primeCursor(term);
}
