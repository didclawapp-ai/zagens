import { readComposerWorkspaceFile } from '../api/client';
import { parseAgentListRow, type AgentListRowMeta } from './agentSpawnMeta';

/** Relative path under workspace for persisted sub-agent snapshots. */
export const SUBAGENT_STATE_REL_PATH = '.zagens/state/subagents.v1.json';
export const LEGACY_SUBAGENT_STATE_REL_PATH = '.deepseek/state/subagents.v1.json';

export type SubagentPollRow = AgentListRowMeta & {
  stepsTaken: number;
  maxSteps: number;
  stepTimeoutMs: number;
  durationMs: number;
  toolsExecuted: number;
  progressStatus?: string;
  stuckSuspected: boolean;
  idleMs: number;
};

function normalizeStatus(raw: unknown): string {
  if (typeof raw === 'string') {
    return raw;
  }
  if (raw && typeof raw === 'object') {
    const keys = Object.keys(raw as Record<string, unknown>);
    if (keys.length === 1) {
      return keys[0]!;
    }
  }
  return 'Running';
}

function parsePollRow(raw: Record<string, unknown>): SubagentPollRow | null {
  const id = String(raw.id ?? raw.agent_id ?? '').trim();
  if (!id) {
    return null;
  }
  const meta = parseAgentListRow({ ...raw, agent_id: id, status: normalizeStatus(raw.status) });
  const stepsTaken = Number(raw.steps_taken ?? 0);
  const maxSteps = Number(raw.max_steps ?? 100);
  const stepTimeoutMs = Number(raw.step_timeout_ms ?? 600_000);
  const durationMs = Number(raw.duration_ms ?? 0);
  const toolsExecuted = Number(raw.tools_executed ?? 0);
  const progressStatus =
    typeof raw.progress_status === 'string' && raw.progress_status.trim()
      ? raw.progress_status.trim()
      : undefined;
  const updatedAtMs = Number(raw.updated_at_ms ?? 0);
  const idleMs =
    updatedAtMs > 0 ? Math.max(0, Date.now() - updatedAtMs) : Number(raw.duration_ms ?? 0);
  const stuckSuspected =
    meta.status === 'Running' && idleMs > stepTimeoutMs + 60_000;

  return {
    ...meta,
    stepsTaken: Number.isFinite(stepsTaken) ? stepsTaken : 0,
    maxSteps: Number.isFinite(maxSteps) ? maxSteps : 100,
    stepTimeoutMs: Number.isFinite(stepTimeoutMs) ? stepTimeoutMs : 600_000,
    durationMs: Number.isFinite(durationMs) ? durationMs : 0,
    toolsExecuted: Number.isFinite(toolsExecuted) ? toolsExecuted : 0,
    ...(progressStatus ? { progressStatus } : {}),
    stuckSuspected,
    idleMs: Number.isFinite(idleMs) ? idleMs : 0,
  };
}

/** Keep rows whose persisted `parent_thread_id` matches the active thread. */
export function filterSubagentRowsForThread(
  rows: SubagentPollRow[],
  threadId: string | null | undefined,
): SubagentPollRow[] {
  const tid = threadId?.trim();
  if (!tid) {
    return [];
  }
  return rows.filter((row) => row.ownerThreadId === tid);
}

/** Read `{workspace}/.zagens/state/subagents.v1.json` (legacy `.deepseek/` fallback). */
export async function fetchSubagentStateFromDisk(
  workspaceRoot: string,
): Promise<SubagentPollRow[]> {
  const root = workspaceRoot.trim();
  if (!root) {
    return [];
  }
  for (const rel of [SUBAGENT_STATE_REL_PATH, LEGACY_SUBAGENT_STATE_REL_PATH]) {
    try {
      const file = await readComposerWorkspaceFile(root, rel);
      const parsed = JSON.parse(file.content) as { agents?: unknown[] };
      if (!Array.isArray(parsed.agents)) {
        return [];
      }
      const rows: SubagentPollRow[] = [];
      for (const entry of parsed.agents) {
        if (!entry || typeof entry !== 'object') {
          continue;
        }
        const row = parsePollRow(entry as Record<string, unknown>);
        if (row) {
          rows.push(row);
        }
      }
      return rows;
    } catch {
      // try legacy path
    }
  }
  return [];
}
