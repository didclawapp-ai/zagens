import type { ToolCardModel } from '../../components/ToolCard';
import {
  DIFF_TOOL_NAMES,
  resolveEditToolPath,
  statsFromEditToolOutput,
} from './diffEntries';

export interface SessionFileChangeRow {
  id: string;
  path: string;
  fileName: string;
  added: number;
  removed: number;
  status: ToolCardModel['status'];
}

interface MessageWithTools {
  id: string;
  tools?: ToolCardModel[];
}

function baseName(path: string): string {
  return path.split(/[/\\]/).pop() ?? path;
}

/** Deduped session edit list (latest row per path), chronological order. */
export function summarizeSessionFileChanges(
  messages: MessageWithTools[],
): SessionFileChangeRow[] {
  const byPath = new Map<string, SessionFileChangeRow>();
  const order: string[] = [];

  const upsert = (path: string, row: SessionFileChangeRow) => {
    if (!byPath.has(path)) {
      order.push(path);
    }
    byPath.set(path, row);
  };

  for (const msg of messages) {
    if (!msg.tools?.length) continue;
    for (const tool of msg.tools) {
      if (!DIFF_TOOL_NAMES.has(tool.name)) continue;
      if (tool.status === 'error') continue;
      const path = resolveEditToolPath(tool);
      if (!path) continue;
      const { added, removed } =
        tool.status === 'running' && !(tool.output ?? '').trim()
          ? { added: 0, removed: 0 }
          : statsFromEditToolOutput(tool.output ?? '');
      upsert(path, {
        id: tool.id,
        path,
        fileName: baseName(path),
        added,
        removed,
        status: tool.status,
      });
    }
  }

  return order.map((path) => byPath.get(path)!);
}
