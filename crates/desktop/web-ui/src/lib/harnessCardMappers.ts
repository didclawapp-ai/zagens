import type { ScratchpadStatus } from '../api/client';
import type { ChecklistPanelPayload } from './panelChannel';
import type { ProgressScrollItem, ProgressState } from './progressScroll';
import type { HarnessTaskGraph } from './types/longHorizon';
import type { AgentState } from '../types/agent';
import { isLikelySubAgentId, truncateObjective } from './agentSpawnMeta';

export interface HarnessCardSummary {
  items: ProgressScrollItem[];
  stat: string;
  progressPct?: number;
}

function promoteFirstPending(items: ProgressScrollItem[]): ProgressScrollItem[] {
  if (items.some((item) => item.progress === 'current')) {
    return items;
  }
  let promoted = false;
  return items.map((item) => {
    if (item.progress !== 'pending' || promoted) {
      return item;
    }
    promoted = true;
    return { ...item, progress: 'current' as ProgressState };
  });
}

export function mapChecklistCardSummary(payload: ChecklistPanelPayload | null): HarnessCardSummary | null {
  if (!payload || payload.items.length === 0) {
    return null;
  }
  const items: ProgressScrollItem[] = payload.items.map((item) => ({
    id: String(item.id),
    progress:
      item.status === 'completed'
        ? 'done'
        : item.status === 'in_progress'
          ? 'current'
          : 'pending',
  }));
  const done = items.filter((item) => item.progress === 'done').length;
  return {
    items: promoteFirstPending(items),
    stat: `${done}/${items.length}`,
    progressPct: payload.completion_pct,
  };
}

export function mapAuditCardSummary(scratchpad: ScratchpadStatus | null): HarnessCardSummary | null {
  if (!scratchpad?.run_id) {
    return null;
  }
  const areas = scratchpad.areas ?? [];
  if (areas.length === 0) {
    const open = scratchpad.findings_open ?? 0;
    return {
      items: [{ id: 'findings', progress: open > 0 ? 'current' : 'done' }],
      stat: `${open}`,
    };
  }
  const items: ProgressScrollItem[] = areas.map((area) => ({
    id: area.id,
    progress:
      area.status === 'done' || area.status === 'deferred'
        ? 'done'
        : area.status === 'in_progress'
          ? 'current'
          : 'pending',
  }));
  const open = items.filter((item) => item.progress !== 'done').length;
  return {
    items: promoteFirstPending(items),
    stat: `${open}`,
  };
}

export function mapLhtCardSummary(graph: HarnessTaskGraph | null): HarnessCardSummary | null {
  if (!graph?.lht_enabled || graph.phases.length === 0) {
    return null;
  }
  const items: ProgressScrollItem[] = graph.phases.map((phase, index) => ({
    id: `${index}-${phase.step}`,
    progress:
      phase.status === 'completed'
        ? 'done'
        : phase.status === 'in_progress'
          ? 'current'
          : 'pending',
  }));
  return {
    items: promoteFirstPending(items),
    stat: `${graph.completion_pct ?? 0}%`,
    progressPct: graph.completion_pct ?? 0,
  };
}

export function mapAgentsCardSummary(agents: AgentState[]): HarnessCardSummary | null {
  const visible = agents.filter((agent) => isLikelySubAgentId(agent.agentId));
  if (visible.length === 0) {
    return null;
  }
  const items: ProgressScrollItem[] = visible.map((agent) => ({
    id: agent.agentId,
    progress:
      agent.status === 'completed' || agent.status === 'interrupted'
        ? 'done'
        : agent.status === 'running' || agent.status === 'spawned'
          ? 'current'
          : 'pending',
  }));
  const done = items.filter((item) => item.progress === 'done').length;
  const open = items.filter((item) => item.progress !== 'done').length;
  return {
    items: promoteFirstPending(items),
    stat: `${done}/${items.length}`,
    progressPct: open > 0 ? Math.round((done / items.length) * 100) : 100,
  };
}

export function harnessCardLineLabel(
  cardId: 'checklist' | 'audit' | 'lht' | 'agents',
  itemId: string,
  sources: {
    checklist: ChecklistPanelPayload | null;
    scratchpad: ScratchpadStatus | null;
    taskGraph: HarnessTaskGraph | null;
    agents: AgentState[];
  },
): string {
  switch (cardId) {
    case 'checklist': {
      const row = sources.checklist?.items.find((item) => String(item.id) === itemId);
      return row?.content ?? itemId;
    }
    case 'audit': {
      if (itemId === 'findings') {
        return sources.scratchpad?.run_id ?? itemId;
      }
      const area = sources.scratchpad?.areas?.find((row) => row.id === itemId);
      return area?.path ?? itemId;
    }
    case 'lht': {
      const index = Number.parseInt(itemId.split('-')[0] ?? '0', 10);
      return sources.taskGraph?.phases[index]?.step ?? itemId;
    }
    case 'agents': {
      const agent = sources.agents.find((row) => row.agentId === itemId);
      return truncateObjective(agent?.objective ?? agent?.agentId ?? itemId, 48);
    }
    default:
      return itemId;
  }
}
