import type { ChecklistPanelPayload } from './panelChannel';
import type { AgentState } from '../types/agent';
import type { TaskSummary } from '../types/automation';

export type InspectorNavActivity = {
  /** Show a small dot beside the sidebar label. */
  active: boolean;
  /** Pulse the dot (in-flight work). */
  pulse: boolean;
};

const SEEN_TASKS_KEY = 'ds-pick:seen-task-ids';

const TASK_ACTIVE_STATUSES = new Set(['queued', 'pending', 'running', 'paused']);

function seenAgentsKey(threadId: string | null): string {
  const id = threadId?.trim();
  return id ? `ds-pick:seen-agent-ids:${id}` : 'ds-pick:seen-agent-ids:';
}

export function loadSeenIds(storageKey: string): Set<string> {
  try {
    const raw = localStorage.getItem(storageKey);
    if (!raw) {
      return new Set();
    }
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) {
      return new Set();
    }
    return new Set(parsed.filter((x): x is string => typeof x === 'string'));
  } catch {
    return new Set();
  }
}

export function saveSeenIds(storageKey: string, ids: Set<string>): void {
  try {
    localStorage.setItem(storageKey, JSON.stringify([...ids]));
  } catch {
    /* ignore */
  }
}

export function countUnreadCompletedAgents(agents: AgentState[], seen: Set<string>): number {
  return agents.filter((a) => a.status === 'completed' && !seen.has(a.agentId)).length;
}

export function countUnreadCompletedTasks(tasks: TaskSummary[], seen: Set<string>): number {
  return tasks.filter((t) => t.status === 'completed' && !seen.has(t.id)).length;
}

export function markAgentsSeen(agents: AgentState[], threadId: string | null): void {
  const key = seenAgentsKey(threadId);
  const seen = loadSeenIds(key);
  for (const a of agents) {
    if (a.status === 'completed') {
      seen.add(a.agentId);
    }
  }
  saveSeenIds(key, seen);
}

export function markTasksSeen(tasks: TaskSummary[]): void {
  const seen = loadSeenIds(SEEN_TASKS_KEY);
  for (const t of tasks) {
    if (t.status === 'completed') {
      seen.add(t.id);
    }
  }
  saveSeenIds(SEEN_TASKS_KEY, seen);
}

export function agentUnreadCount(agents: AgentState[], threadId: string | null): number {
  return countUnreadCompletedAgents(agents, loadSeenIds(seenAgentsKey(threadId)));
}

export function taskUnreadCount(tasks: TaskSummary[]): number {
  return countUnreadCompletedTasks(tasks, loadSeenIds(SEEN_TASKS_KEY));
}

function seenChecklistKey(threadId: string | null): string {
  const id = threadId?.trim();
  return id ? `ds-pick:seen-checklist:${id}` : 'ds-pick:seen-checklist:';
}

export function checklistFingerprint(data: ChecklistPanelPayload): string {
  const items = data.items.map((i) => `${i.id}:${i.status}`).join(',');
  return `${data.completion_pct}|${data.in_progress_id ?? ''}|${items}`;
}

export function loadSeenChecklistFingerprint(threadId: string | null): string | null {
  try {
    return localStorage.getItem(seenChecklistKey(threadId));
  } catch {
    return null;
  }
}

export function markChecklistSeen(data: ChecklistPanelPayload | null, threadId: string | null): void {
  if (!data || data.items.length === 0) {
    try {
      localStorage.removeItem(seenChecklistKey(threadId));
    } catch {
      /* ignore */
    }
    return;
  }
  try {
    localStorage.setItem(seenChecklistKey(threadId), checklistFingerprint(data));
  } catch {
    /* ignore */
  }
}

export function taskNavActivity(
  tasks: TaskSummary[],
  panelOpen: boolean,
): InspectorNavActivity {
  if (panelOpen) {
    return { active: false, pulse: false };
  }
  const running = tasks.some((t) => TASK_ACTIVE_STATUSES.has(t.status));
  const unread = taskUnreadCount(tasks) > 0;
  return { active: running || unread, pulse: running };
}

export function agentNavActivity(
  agents: AgentState[],
  threadId: string | null,
  panelOpen: boolean,
): InspectorNavActivity {
  if (panelOpen) {
    return { active: false, pulse: false };
  }
  const running = agents.some((a) => a.status === 'spawned' || a.status === 'running');
  const unread = agentUnreadCount(agents, threadId) > 0;
  return { active: running || unread, pulse: running };
}

export function checklistNavActivity(
  data: ChecklistPanelPayload | null,
  threadId: string | null,
  panelOpen: boolean,
): InspectorNavActivity {
  if (panelOpen || !data || data.items.length === 0) {
    return { active: false, pulse: false };
  }
  const seen = loadSeenChecklistFingerprint(threadId);
  const fp = checklistFingerprint(data);
  const hasOpenWork = data.items.some(
    (i) => i.status === 'pending' || i.status === 'in_progress',
  );
  const changed = seen !== fp;
  return {
    active: hasOpenWork || changed,
    pulse: hasOpenWork || data.in_progress_id != null,
  };
}
