import type { ToolCardModel } from '../../components/ToolCard';
import { normalizeWorkspaceRelPath } from '../openWorkspaceFile';

export const DIFF_TOOL_NAMES = new Set(['edit_file', 'apply_patch', 'write_file']);

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
  const trimmed = text.trim();
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
  } catch {
    /* plain string path */
  }
  const trimmed = input.trim();
  if (trimmed && !trimmed.startsWith('{')) return trimmed;
  return undefined;
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
      const fileName = parseFileNameFromToolInput(tool.input) ?? tool.name;
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
    const rel = normalizeWorkspaceRelPath(e.fileName);
    if (rel) paths.add(rel);
  }
  return [...paths];
}
