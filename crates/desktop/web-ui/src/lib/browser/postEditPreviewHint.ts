/**
 * Soft toast after edits that are actually previewable in the Browser pane.
 * Do not fire for every write_file (JSON/TS/Rust noise).
 */

import { isEditToolForPreviewHint } from './browserPrefs';
import {
  extractPathsFromEditToolOutput,
  parseWriteSummaryStats,
} from '../diff/diffEntries';

/** Extensions / config that make sense to open in the desktop Browser pane. */
const PREVIEW_EXTENSIONS = new Set([
  '.html',
  '.htm',
  '.xhtml',
  '.css',
  '.svg',
]);

/** Normalize and test whether a workspace path is Browser-preview relevant. */
export function isBrowserPreviewRelevantPath(path: string): boolean {
  const norm = path.trim().replace(/\\/g, '/');
  if (!norm) return false;
  const lower = norm.toLowerCase();
  const base = lower.split('/').pop() ?? lower;
  if (base === 'preview.json' || lower.endsWith('/.zagens/preview.json')) {
    return true;
  }
  const dot = base.lastIndexOf('.');
  if (dot < 0) return false;
  return PREVIEW_EXTENSIONS.has(base.slice(dot));
}

export function editOutputSuggestsBrowserPreview(output: string): boolean {
  const paths = extractPathsFromEditToolOutput(output);
  if (paths.length === 0) return false;
  return paths.some(isBrowserPreviewRelevantPath);
}

/** After offering once, suppress repeats during a burst of agent writes. */
export const POST_EDIT_PREVIEW_HINT_COOLDOWN_MS = 5 * 60 * 1000;

let lastOfferedAt = 0;

export function resetPostEditPreviewHintCooldownForTests(): void {
  lastOfferedAt = 0;
}

export function shouldShowPostEditPreviewHint(
  toolName: string,
  output: string,
  opts?: { now?: number; cooldownMs?: number },
): boolean {
  if (!isEditToolForPreviewHint(toolName)) return false;
  if (!editOutputSuggestsBrowserPreview(output)) return false;

  const now = opts?.now ?? Date.now();
  const cooldown = opts?.cooldownMs ?? POST_EDIT_PREVIEW_HINT_COOLDOWN_MS;
  if (lastOfferedAt > 0 && now - lastOfferedAt < cooldown) {
    return false;
  }
  lastOfferedAt = now;
  return true;
}

// Re-export for tests that assert path parsing from tool output.
export { extractPathsFromEditToolOutput, parseWriteSummaryStats };
