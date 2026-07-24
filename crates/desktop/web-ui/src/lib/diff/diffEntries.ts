import type { ToolCardModel } from '../../components/ToolCard';
import { normalizeWorkspaceRelPath } from '../openWorkspaceFile';

export const DIFF_TOOL_NAMES = new Set(['edit_file', 'apply_patch', 'write_file']);

const TOOL_NAME_PATH_BLOCKLIST = new Set([
  'write_file',
  'edit_file',
  'apply_patch',
  'batch_edit',
]);

const WRITE_SUMMARY_RE =
  /(?:Wrote|Created)\s+(\d+)\s+bytes(?:\s+\((\d+)\s+lines?\))?\s+to\s+(.+?)(?:\r?\n|$)/gi;
const REPLACED_SUMMARY_RE =
  /Replaced\s+(?:\d+\s+occurrence\(s\)|line\s+\d+)\s+in\s+(.+?)(?:\s+→|\r?\n|$)/gi;
const DIFF_PLUS_RE = /^\+\+\+\s+(?:b\/)?(.+)$/gm;
const DIFF_GIT_RE = /^diff --git\s+a\/(.+?)\s+b\/(.+)$/gm;
const FACT_PATH_RE = /^- fact: path=(.+)$/gm;
const CITE_PATH_RE = /^- cite: ([^\s:]+)(?::\d+(?:-\d+)?)?$/gm;

export interface DiffEntry {
  id: string;
  diffText: string;
  fileName: string;
  toolName: string;
  messageId: string;
  status: ToolCardModel['status'];
  added: number;
  removed: number;
}

function stripLargeFilePreview(text: string): string {
  const idx = text.search(/\[diff omitted[^\]]*\]/i);
  return idx >= 0 ? text.slice(0, idx).trim() : text;
}

function normalizeEditPath(path: string): string {
  let p = path.trim().replace(/^["'`]+|["'`]+$/g, '');
  if (p.startsWith('\\\\?\\')) p = p.slice(4);
  p = p.replace(/\\/g, '/');
  if (p.startsWith('b/') || p.startsWith('a/')) p = p.slice(2);
  return normalizeWorkspaceRelPath(p) ?? p.replace(/^\.\/+/, '');
}

function baseName(path: string): string {
  return path.split(/[/\\]/).pop() ?? path;
}

function isPlausibleEditPath(path: string, toolName: string): boolean {
  const norm = path.trim();
  if (!norm) return false;
  if (norm === toolName) return false;
  const base = baseName(norm).toLowerCase();
  if (TOOL_NAME_PATH_BLOCKLIST.has(base)) return false;
  return true;
}

/** Count +/- lines in a unified diff (excludes ---/+++/@@ headers). */
export function countUnifiedDiffLines(diffText: string): { added: number; removed: number } {
  let added = 0;
  let removed = 0;
  for (const line of diffText.split(/\r?\n/)) {
    if (!line) continue;
    const marker = line[0];
    if (marker === '+' && !line.startsWith('+++')) added += 1;
    else if (marker === '-' && !line.startsWith('---')) removed += 1;
  }
  return { added, removed };
}

export function looksLikeDiff(text: string): boolean {
  return (
    /^--- /m.test(text) ||
    /^\+\+\+ /m.test(text) ||
    /^@@ .* @@/m.test(text) ||
    /^diff --git /m.test(text)
  );
}

/** Pull unified diff from tool output that may be prefixed with progress lines. */
export function extractUnifiedDiff(text: string): string | null {
  const trimmed = stripLargeFilePreview(text.trim());
  if (!trimmed) return null;
  if (looksLikeDiff(trimmed)) return trimmed;
  const markers = [/^--- /m, /^diff --git /m];
  for (const re of markers) {
    const m = trimmed.match(re);
    if (m?.index != null) {
      const slice = trimmed.slice(m.index).trim();
      if (looksLikeDiff(slice)) return slice;
    }
  }
  return null;
}

export function parseFileNameFromToolInput(input: string): string | undefined {
  try {
    const j = JSON.parse(input) as Record<string, unknown>;
    if (typeof j.path === 'string' && j.path.trim()) return j.path.trim();
    if (typeof j.file_path === 'string' && j.file_path.trim()) return j.file_path.trim();
    if (typeof j.file === 'string' && j.file.trim()) return j.file.trim();
  } catch {
    /* plain string path */
  }
  const trimmed = input.trim();
  if (trimmed && !trimmed.startsWith('{')) return trimmed;
  return undefined;
}

/** Pull edited paths from write/edit/patch tool output text. */
export function extractPathsFromEditToolOutput(output: string): string[] {
  const found = new Set<string>();
  const add = (raw: string | undefined) => {
    if (!raw) return;
    const p = normalizeEditPath(raw);
    if (!p || p === '/dev/null') return;
    found.add(p);
  };

  const body = stripLargeFilePreview(output);
  for (const m of body.matchAll(WRITE_SUMMARY_RE)) add(m[3]);
  for (const m of body.matchAll(REPLACED_SUMMARY_RE)) add(m[1]);
  for (const m of body.matchAll(DIFF_PLUS_RE)) add(m[1]);
  for (const m of body.matchAll(DIFF_GIT_RE)) add(m[2] ?? m[1]);
  for (const m of body.matchAll(FACT_PATH_RE)) add(m[1]);
  for (const m of body.matchAll(CITE_PATH_RE)) add(m[1]);

  return [...found];
}

export function parseWriteSummaryStats(
  output: string,
): { bytes: number; lines: number | null; path: string } | null {
  const body = stripLargeFilePreview(output);
  const match = /(?:Wrote|Created)\s+(\d+)\s+bytes(?:\s+\((\d+)\s+lines?\))?\s+to\s+(.+?)(?:\r?\n|$)/i.exec(
    body,
  );
  if (!match) return null;
  return {
    bytes: Number.parseInt(match[1] ?? '0', 10),
    lines: match[2] ? Number.parseInt(match[2], 10) : null,
    path: match[3]?.trim() ?? '',
  };
}

export function resolveEditToolPath(
  tool: Pick<ToolCardModel, 'name' | 'input' | 'output'>,
): string | null {
  const fromInput = parseFileNameFromToolInput(tool.input ?? '');
  if (fromInput && isPlausibleEditPath(fromInput, tool.name)) {
    return normalizeEditPath(fromInput);
  }
  for (const path of extractPathsFromEditToolOutput(tool.output ?? '')) {
    if (isPlausibleEditPath(path, tool.name)) return normalizeEditPath(path);
  }
  return null;
}

export function statsFromEditToolOutput(output: string): { added: number; removed: number } {
  const diffText = extractUnifiedDiff(output);
  if (diffText) return countUnifiedDiffLines(diffText);
  const summary = parseWriteSummaryStats(output);
  if (summary) {
    return {
      added: summary.lines ?? Math.max(1, Math.round(summary.bytes / 80)),
      removed: 0,
    };
  }
  return { added: 0, removed: 0 };
}

export function entryLabel(entry: DiffEntry): string {
  const base = entry.fileName.split(/[/\\]/).pop() ?? entry.fileName;
  if (entry.status === 'running') return `${base} …`;
  return base;
}

interface MessageWithTools {
  id: string;
  tools?: ToolCardModel[];
}

/** Collect unified diffs from edit_file / apply_patch / write_file tool results (same heuristics as chat ToolCard). */
export function extractDiffEntries(messages: MessageWithTools[]): DiffEntry[] {
  const entries: DiffEntry[] = [];
  for (const msg of messages) {
    if (!msg.tools?.length) continue;
    for (const tool of msg.tools) {
      if (!DIFF_TOOL_NAMES.has(tool.name)) continue;
      const diffText = extractUnifiedDiff(tool.output ?? '');
      if (!diffText) continue;
      const fileName = resolveEditToolPath(tool);
      if (!fileName) continue;
      const { added, removed } = countUnifiedDiffLines(diffText);
      entries.push({
        id: tool.id,
        diffText,
        fileName,
        toolName: tool.name,
        messageId: msg.id,
        status: tool.status,
        added,
        removed,
      });
    }
  }
  return entries;
}

/** Normalized workspace-relative paths from tool diffs in the session (Office「本轮变更」筛选). */
export function extractDiffRelPaths(messages: MessageWithTools[]): string[] {
  const paths = new Set<string>();
  for (const e of extractDiffEntries(messages)) {
    paths.add(normalizeEditPath(e.fileName));
  }
  return [...paths];
}
