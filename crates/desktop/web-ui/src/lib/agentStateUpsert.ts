import type { AgentState } from '../types/agent';
import {
  isLikelySubAgentId,
  mergeAgentMeta,
  type AgentListRowMeta,
  type AgentSpawnMeta,
} from './agentSpawnMeta';

export function defaultAgentState(agentId: string): AgentState {
  return {
    agentId,
    status: 'spawned',
    toolCalls: [],
    resultSummary: null,
    tokens: 0,
    spawnedAt: Date.now(),
    completedAt: null,
  };
}

export function metaFromSpawn(meta: AgentSpawnMeta | null | undefined): Partial<AgentState> {
  if (!meta) {
    return {};
  }
  return {
    objective: meta.objective,
    agentType: meta.agentType,
    role: meta.role,
    taskId: meta.taskId,
  };
}

export function metaFromListRow(row: AgentListRowMeta): Partial<AgentState> {
  return {
    ...(row.ownerThreadId ? { ownerThreadId: row.ownerThreadId } : {}),
    ...(row.objective ? { objective: row.objective } : {}),
    ...(row.agentType ? { agentType: row.agentType } : {}),
    ...(row.role ? { role: row.role } : {}),
    ...(row.taskId ? { taskId: row.taskId } : {}),
    ...(row.nickname ? { nickname: row.nickname } : {}),
    ...(row.progressStatus ? { progressStatus: row.progressStatus } : {}),
    ...(row.stepsTaken !== undefined ? { stepsTaken: row.stepsTaken } : {}),
    ...(row.maxSteps !== undefined ? { maxSteps: row.maxSteps } : {}),
    ...(row.stepTimeoutMs !== undefined ? { stepTimeoutMs: row.stepTimeoutMs } : {}),
    ...(row.stuckSuspected !== undefined ? { stuckSuspected: row.stuckSuspected } : {}),
    ...(row.idleMs !== undefined ? { idleMs: row.idleMs } : {}),
    ...(row.toolsExecuted !== undefined ? { toolsExecuted: row.toolsExecuted } : {}),
    ...(row.durationMs !== undefined ? { durationMs: row.durationMs } : {}),
  };
}

export function upsertAgentInList(
  prev: AgentState[],
  agentId: string,
  patch: Partial<AgentState> & { status?: AgentState['status'] },
): AgentState[] {
  if (!isLikelySubAgentId(agentId)) {
    return prev;
  }
  const idx = prev.findIndex((a) => a.agentId === agentId);
  const base = idx >= 0 ? prev[idx]! : defaultAgentState(agentId);
  let merged = mergeAgentMeta(base, patch);
  if (patch.status !== undefined) {
    merged = { ...merged, status: patch.status };
  }
  if (patch.toolCalls !== undefined) {
    merged = { ...merged, toolCalls: patch.toolCalls };
  }
  if (patch.resultSummary !== undefined) {
    merged = { ...merged, resultSummary: patch.resultSummary };
  }
  if (patch.completedAt !== undefined) {
    merged = { ...merged, completedAt: patch.completedAt };
  }
  if (patch.tokens !== undefined) {
    merged = { ...merged, tokens: patch.tokens };
  }
  if (patch.spawnedAt !== undefined) {
    merged = { ...merged, spawnedAt: patch.spawnedAt };
  }
  if (patch.ownerThreadId !== undefined) {
    merged = { ...merged, ownerThreadId: patch.ownerThreadId };
  }
  if (idx >= 0) {
    const next = [...prev];
    next[idx] = merged;
    return next;
  }
  return [...prev, merged];
}
