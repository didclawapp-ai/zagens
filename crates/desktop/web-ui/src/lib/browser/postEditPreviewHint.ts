/**
 * Soft toast after edits that are actually previewable in the Browser pane.
 * Do not fire for every write_file (JSON/TS/Rust noise).
 */

import { isEditToolForPreviewHint } from './browserPrefs';

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

const WRITE_SUMMARY_RE =
  /(?:Wrote|Created)\s+\d+\s+bytes(?:\s+\(\d+\s+lines?\))?\s+to\s+(.+?)(?:\r?\n|$)/gi;
const REPLACED_SUMMARY_RE =
  /Replaced\s+(?:\d+\s+occurrence\(s\)|line\s+\d+)\s+in\s+(.+?)(?:\s+→|\r?\n|$)/gi;
const DIFF_PLUS_RE = /^\+\+\+\s+(?:b\/)?(.+)$/gm;
const DIFF_GIT_RE = /^diff --git\s+a\/(.+?)\s+b\/(.+)$/gm;
const FACT_PATH_RE = /^- fact: path=(.+)$/gm;
const CITE_PATH_RE = /^- cite: ([^\s:]+)(?::\d+(?:-\d+)?)?$/gm;

/** Pull edited paths from write/edit/patch tool output text. */
export function extractPathsFromEditToolOutput(output: string): string[] {
  const found = new Set<string>();
  const add = (raw: string | undefined) => {
    if (!raw) return;
    let p = raw.trim().replace(/^["'`]+|["'`]+$/g, '');
    if (p.startsWith('\\\\?\\')) p = p.slice(4);
    p = p.replace(/\\/g, '/');
    if (p.startsWith('b/') || p.startsWith('a/')) p = p.slice(2);
    if (!p || p === '/dev/null') return;
    found.add(p);
  };

  for (const m of output.matchAll(WRITE_SUMMARY_RE)) add(m[1]);
  for (const m of output.matchAll(REPLACED_SUMMARY_RE)) add(m[1]);
  for (const m of output.matchAll(DIFF_PLUS_RE)) add(m[1]);
  for (const m of output.matchAll(DIFF_GIT_RE)) add(m[2] ?? m[1]);
  for (const m of output.matchAll(FACT_PATH_RE)) add(m[1]);
  for (const m of output.matchAll(CITE_PATH_RE)) add(m[1]);

  return [...found];
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
