/**
 * Panel channel (C): SSE `panel.*` events → window CustomEvents for checklist / scratchpad / context.
 * Reduces B-channel HTTP polling while a turn is streaming.
 */

import type { ScratchpadStatus } from '../api/client';
import type { ThreadContextSnapshot } from './contextUsage';
import type { HarnessTaskGraph } from './types/longHorizon';

export const PANEL_SCRATCHPAD_EVENT = 'deepseek-panel-scratchpad';
export const PANEL_CHECKLIST_EVENT = 'deepseek-panel-checklist';
export const PANEL_CONTEXT_EVENT = 'deepseek-panel-context';
export const PANEL_TASK_GRAPH_EVENT = 'deepseek-panel-task-graph';
export const HARNESS_CYCLE_ADVANCED_EVENT = 'deepseek-harness-cycle-advanced';

export interface TaskGraphPanelPayload {
  task_graph: HarnessTaskGraph;
}

export interface ChecklistPanelPayload {
  items: Array<{
    id: number;
    content: string;
    status: 'pending' | 'in_progress' | 'completed';
  }>;
  completion_pct: number;
  in_progress_id: number | null;
}

export function dispatchPanelScratchpad(scratchpad: ScratchpadStatus | null): void {
  window.dispatchEvent(
    new CustomEvent(PANEL_SCRATCHPAD_EVENT, { detail: scratchpad }),
  );
}

export function dispatchPanelChecklist(checklist: ChecklistPanelPayload | null): void {
  window.dispatchEvent(
    new CustomEvent(PANEL_CHECKLIST_EVENT, { detail: checklist }),
  );
}

export function dispatchPanelContext(context: ThreadContextSnapshot): void {
  window.dispatchEvent(
    new CustomEvent(PANEL_CONTEXT_EVENT, { detail: context }),
  );
}

export function dispatchPanelTaskGraph(task_graph: HarnessTaskGraph): void {
  window.dispatchEvent(
    new CustomEvent(PANEL_TASK_GRAPH_EVENT, { detail: { task_graph } }),
  );
}

export function dispatchHarnessCycleAdvanced(detail: {
  from: number;
  to: number;
}): void {
  window.dispatchEvent(
    new CustomEvent(HARNESS_CYCLE_ADVANCED_EVENT, { detail }),
  );
}

/** Normalize runtime checklist JSON into panel shape. */
export function normalizeChecklistPayload(raw: unknown): ChecklistPanelPayload | null {
  if (!raw || typeof raw !== 'object') {
    return null;
  }
  const o = raw as Record<string, unknown>;
  const itemsRaw = o.items;
  if (!Array.isArray(itemsRaw) || itemsRaw.length === 0) {
    return null;
  }
  const items = itemsRaw.map((row) => {
    const r = row as Record<string, unknown>;
    const status = String(r.status ?? 'pending');
    const normalizedStatus =
      status === 'completed' || status === 'in_progress' ? status : 'pending';
    return {
      id: Number(r.id ?? 0),
      content: String(r.content ?? ''),
      status: normalizedStatus as ChecklistPanelPayload['items'][0]['status'],
    };
  });
  const total = items.length;
  const completed = items.filter((i) => i.status === 'completed').length;
  const inProgress = items.find((i) => i.status === 'in_progress');
  return {
    items,
    completion_pct:
      typeof o.completion_pct === 'number'
        ? o.completion_pct
        : total > 0
          ? Math.round((completed / total) * 100)
          : 0,
    in_progress_id:
      typeof o.in_progress_id === 'number'
        ? o.in_progress_id
        : inProgress?.id ?? null,
  };
}
