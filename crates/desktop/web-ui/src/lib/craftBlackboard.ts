/**
 * CRAFT blackboard summaries for AgentPanel (B-L3).
 * Data from `GET /v1/blackboards` + detail endpoints.
 */

import { fetchBlackboardDetail, fetchBlackboardList } from '../api/client';

export const CRAFT_BLACKBOARD_EVENT = 'deepseek-craft-blackboard-changed';

export interface CraftBlackboardTaskSummary {
  taskId: string;
  explorerDone: boolean;
  implementerRounds: number;
  reviewerVerdict: string | null;
  verifierSummary: string | null;
}

function verdictLabel(raw: unknown): string | null {
  if (raw == null) return null;
  if (typeof raw === 'string') {
    const t = raw.trim();
    return t.length > 0 ? t : null;
  }
  if (typeof raw === 'object') {
    const keys = Object.keys(raw as Record<string, unknown>);
    if (keys.length === 1) return keys[0] ?? null;
  }
  return null;
}

export function summarizeBlackboard(
  taskId: string,
  board: unknown,
): CraftBlackboardTaskSummary {
  const d =
    board && typeof board === 'object' ? (board as Record<string, unknown>) : {};

  const explorer = d.explorer;
  const implementer = d.implementer;
  const implementerObj =
    implementer && typeof implementer === 'object'
      ? (implementer as Record<string, unknown>)
      : null;
  const rounds = implementerObj?.rounds;
  const implementerRounds = Array.isArray(rounds) ? rounds.length : 0;

  const reviewer =
    d.reviewer && typeof d.reviewer === 'object'
      ? (d.reviewer as Record<string, unknown>)
      : null;
  const verifier =
    d.verifier && typeof d.verifier === 'object'
      ? (d.verifier as Record<string, unknown>)
      : null;

  return {
    taskId,
    explorerDone: explorer != null && typeof explorer === 'object',
    implementerRounds,
    reviewerVerdict: verdictLabel(reviewer?.verdict),
    verifierSummary:
      typeof verifier?.summary === 'string' && verifier.summary.trim()
        ? verifier.summary.trim()
        : null,
  };
}

export async function fetchCraftBlackboardTasks(
  workspace?: string,
): Promise<CraftBlackboardTaskSummary[]> {
  const ids = await fetchBlackboardList(workspace);
  if (ids.length === 0) {
    return [];
  }
  const summaries = await Promise.all(
    ids.map(async (taskId) => {
      try {
        const board = await fetchBlackboardDetail(taskId, workspace);
        return summarizeBlackboard(taskId, board);
      } catch {
        return {
          taskId,
          explorerDone: false,
          implementerRounds: 0,
          reviewerVerdict: null,
          verifierSummary: null,
        };
      }
    }),
  );
  return summaries.sort((a, b) => a.taskId.localeCompare(b.taskId));
}

export function notifyCraftBlackboardChanged(): void {
  window.dispatchEvent(new CustomEvent(CRAFT_BLACKBOARD_EVENT));
}

export function onCraftBlackboardChanged(handler: () => void): () => void {
  window.addEventListener(CRAFT_BLACKBOARD_EVENT, handler);
  return () => window.removeEventListener(CRAFT_BLACKBOARD_EVENT, handler);
}
