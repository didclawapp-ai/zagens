import type { AgentState } from '../types/agent';

const SPAWN_TOOL_NAMES = new Set(['agent_spawn', 'spawn_agent', 'delegate_to_agent']);

/** Runtime sub-agent ids (`agent_*`), not parent tool-call ids (`call_*`). */
export function isLikelySubAgentId(id: string): boolean {
  const t = id.trim();
  return /^agent[_-]/i.test(t);
}

export function isAgentSpawnToolName(name: string): boolean {
  return SPAWN_TOOL_NAMES.has(name);
}

/** Fields from `agent_spawn` tool input (prompt / type / task_id). */
export interface AgentSpawnMeta {
  objective: string;
  agentType?: string;
  role?: string;
  taskId?: string;
}

export function parseAgentSpawnInput(input: unknown): AgentSpawnMeta | null {
  if (input == null) {
    return null;
  }
  let raw: Record<string, unknown>;
  if (typeof input === 'string') {
    const trimmed = input.trim();
    if (!trimmed) {
      return null;
    }
    try {
      raw = JSON.parse(trimmed) as Record<string, unknown>;
    } catch {
      return { objective: trimmed };
    }
  } else if (typeof input === 'object') {
    raw = input as Record<string, unknown>;
  } else {
    return null;
  }

  const objective = pickObjective(raw);
  if (!objective) {
    return null;
  }

  const agentType = pickString(raw, ['type', 'agent_type', 'agent_name']);
  const role = pickString(raw, ['role', 'agent_role']);
  const taskId = pickString(raw, ['task_id', 'scratchpad_run_id']);

  return {
    objective,
    ...(agentType ? { agentType } : {}),
    ...(role ? { role } : {}),
    ...(taskId ? { taskId } : {}),
  };
}

function pickObjective(raw: Record<string, unknown>): string {
  for (const key of ['prompt', 'message', 'objective', 'description']) {
    const v = raw[key];
    if (typeof v === 'string' && v.trim()) {
      return v.trim();
    }
  }
  return '';
}

function pickString(raw: Record<string, unknown>, keys: string[]): string | undefined {
  for (const key of keys) {
    const v = raw[key];
    if (typeof v === 'string' && v.trim()) {
      return v.trim();
    }
  }
  return undefined;
}

export function agentTypeLabel(agentType: string | undefined): string | null {
  if (!agentType?.trim()) {
    return null;
  }
  return agentType.replace(/_/g, ' ').trim();
}

/** Merge spawn / list metadata into an agent row (non-empty wins). */
export function mergeAgentMeta(
  agent: AgentState,
  patch: Partial<
    Pick<
      AgentState,
      | 'objective'
      | 'agentType'
      | 'role'
      | 'taskId'
      | 'nickname'
      | 'progressStatus'
      | 'stepsTaken'
      | 'maxSteps'
      | 'stepTimeoutMs'
      | 'stuckSuspected'
      | 'idleMs'
    >
  >,
): AgentState {
  const next = { ...agent };
  if (patch.objective?.trim()) {
    next.objective = patch.objective.trim();
  }
  if (patch.agentType?.trim()) {
    next.agentType = patch.agentType.trim();
  }
  if (patch.role?.trim()) {
    next.role = patch.role.trim();
  }
  if (patch.taskId?.trim()) {
    next.taskId = patch.taskId.trim();
  }
  if (patch.nickname?.trim()) {
    next.nickname = patch.nickname.trim();
  }
  if (patch.progressStatus?.trim()) {
    next.progressStatus = patch.progressStatus.trim();
  }
  if (patch.stepsTaken !== undefined) {
    next.stepsTaken = patch.stepsTaken;
  }
  if (patch.maxSteps !== undefined) {
    next.maxSteps = patch.maxSteps;
  }
  if (patch.stepTimeoutMs !== undefined) {
    next.stepTimeoutMs = patch.stepTimeoutMs;
  }
  if (patch.stuckSuspected !== undefined) {
    next.stuckSuspected = patch.stuckSuspected;
  }
  if (patch.idleMs !== undefined) {
    next.idleMs = patch.idleMs;
  }
  return next;
}

export interface AgentListRowMeta {
  id: string;
  status: string;
  /** Parent runtime thread (`parent_thread_id` from runtime / disk). */
  ownerThreadId?: string;
  objective?: string;
  agentType?: string;
  role?: string;
  taskId?: string;
  nickname?: string;
  stepsTaken?: number;
  maxSteps?: number;
  stepTimeoutMs?: number;
  progressStatus?: string;
  stuckSuspected?: boolean;
  idleMs?: number;
}

/** Parse one entry from runtime `agent.list` / SubAgentResult JSON. */
export function parseAgentListRow(raw: Record<string, unknown>): AgentListRowMeta {
  const id = String(raw.agent_id ?? raw.id ?? '').trim();
  const assignment =
    raw.assignment && typeof raw.assignment === 'object'
      ? (raw.assignment as Record<string, unknown>)
      : undefined;
  const objective =
    (typeof assignment?.objective === 'string' ? assignment.objective.trim() : '') ||
    (typeof raw.prompt === 'string' ? raw.prompt.trim() : '');
  const role =
    typeof assignment?.role === 'string' && assignment.role.trim()
      ? assignment.role.trim()
      : undefined;
  const agentType = pickString(raw, ['agent_type', 'type']);
  const nickname =
    typeof raw.nickname === 'string' && raw.nickname.trim() ? raw.nickname.trim() : undefined;
  const taskId = typeof raw.task_id === 'string' && raw.task_id.trim() ? raw.task_id.trim() : undefined;
  const stepsTaken = Number(raw.steps_taken ?? 0);
  const maxSteps = Number(raw.max_steps ?? 0);
  const stepTimeoutMs = Number(raw.step_timeout_ms ?? 0);
  const progressStatus =
    typeof raw.progress_status === 'string' && raw.progress_status.trim()
      ? raw.progress_status.trim()
      : undefined;
  const idleMs = Number(raw.idle_ms ?? 0);
  const stuckSuspected = raw.stuck_suspected === true;
  const ownerThreadId = pickString(raw, ['parent_thread_id', 'parentThreadId']);

  return {
    id,
    status: normalizeListStatus(raw.status),
    ...(ownerThreadId ? { ownerThreadId } : {}),
    ...(objective ? { objective } : {}),
    ...(agentType ? { agentType } : {}),
    ...(role ? { role } : {}),
    ...(taskId ? { taskId } : {}),
    ...(nickname ? { nickname } : {}),
    ...(raw.steps_taken !== undefined && Number.isFinite(stepsTaken) ? { stepsTaken } : {}),
    ...(Number.isFinite(maxSteps) && maxSteps > 0 ? { maxSteps } : {}),
    ...(Number.isFinite(stepTimeoutMs) && stepTimeoutMs > 0 ? { stepTimeoutMs } : {}),
    ...(progressStatus ? { progressStatus } : {}),
    ...(stuckSuspected ? { stuckSuspected: true } : {}),
    ...(Number.isFinite(idleMs) && idleMs > 0 ? { idleMs } : {}),
  };
}

function normalizeListStatus(status: unknown): string {
  if (typeof status === 'string') {
    return status;
  }
  if (status && typeof status === 'object') {
    const keys = Object.keys(status as Record<string, unknown>);
    if (keys.length === 1) {
      const k = keys[0] ?? 'Running';
      if (k === 'Interrupted' || k === 'Failed') {
        const inner = (status as Record<string, unknown>)[k];
        if (typeof inner === 'string' && inner.trim()) {
          return `Interrupted`;
        }
      }
      return k;
    }
  }
  return 'Running';
}

export function truncateObjective(text: string, maxLen = 160): string {
  const t = text.replace(/\s+/g, ' ').trim();
  if (t.length <= maxLen) {
    return t;
  }
  return `${t.slice(0, maxLen - 1)}…`;
}
