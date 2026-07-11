import { readComposerWorkspaceFile } from '../api/client';

/** Relative paths under workspace for sub-agent step journals. */
export const SUBAGENT_JOURNAL_REL = (agentId: string) =>
  `.zagens/state/subagent-journals/${agentId}.json`;
export const LEGACY_SUBAGENT_JOURNAL_REL = (agentId: string) =>
  `.deepseek/state/subagent-journals/${agentId}.json`;

export async function fetchSubagentJournalJson(
  workspaceRoot: string,
  agentId: string,
): Promise<string | null> {
  const root = workspaceRoot.trim();
  const id = agentId.trim();
  if (!root || !id) {
    return null;
  }
  for (const rel of [SUBAGENT_JOURNAL_REL(id), LEGACY_SUBAGENT_JOURNAL_REL(id)]) {
    try {
      const file = await readComposerWorkspaceFile(root, rel);
      if (file.content?.trim()) {
        return file.content;
      }
    } catch {
      // try legacy / missing
    }
  }
  return null;
}

export function downloadTextFile(filename: string, content: string): void {
  const blob = new Blob([content], { type: 'application/json;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}
