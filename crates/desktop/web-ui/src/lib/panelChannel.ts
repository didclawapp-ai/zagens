/**
 * Panel channel (C): SSE `panel.*` events → window CustomEvents for checklist / scratchpad / context.
 * Reduces B-channel HTTP polling while a turn is streaming.
 *
 * Multi-session thread routing (P0.6 hardening): each dispatcher accepts an
 * optional `originThreadId`. When provided AND a panel active-thread has been
 * registered via `setPanelActiveThreadId`, the event is silently dropped if
 * `originThreadId !== activePanelThreadId`. This is a defensive guard — the
 * primary isolation is the `isBackground` early-return in `useTurnSend.applyNorm`,
 * but this guard prevents future call sites that forget the active-view check
 * from leaking background-thread panel state into the active UI.
 */

import type { ScratchpadStatus } from '../api/client';
import type { ContextUsageBreakdown, ThreadContextSnapshot } from './contextUsage';
import type { HarnessTaskGraph } from './types/longHorizon';

export const PANEL_SCRATCHPAD_EVENT = 'deepseek-panel-scratchpad';
export const PANEL_CHECKLIST_EVENT = 'deepseek-panel-checklist';
export const PANEL_CONTEXT_EVENT = 'deepseek-panel-context';
export const PANEL_CONTEXT_USAGE_EVENT = 'deepseek-panel-context-usage';
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

/**
 * Module-level active thread for panel dispatch filtering.
 * Synced from the registry via `setPanelActiveThreadId` (called from
 * `useTurnSend` / `App.tsx` whenever `activeThreadId` changes).
 */
let activePanelThreadId: string | null = null;

/**
 * Register the thread whose panel slice is currently rendered in the active
 * view. Pass `null` to disable filtering (e.g. during teardown).
 */
export function setPanelActiveThreadId(threadId: string | null): void {
  activePanelThreadId = threadId?.trim() || null;
}

/** Read the current panel active thread (mainly for tests). */
export function getPanelActiveThreadId(): string | null {
  return activePanelThreadId;
}

/**
 * Returns true if a panel event from `originThreadId` should be dispatched.
 * - No `originThreadId` → always dispatch (backward-compatible, used by
 *   `sessionPanelReattach` which restores a slice right after navigation).
 * - `originThreadId` provided but no active registered → dispatch (filter off).
 * - `originThreadId` provided and active registered and they differ → drop.
 */
/** Shared active-view guard for panel dispatchers and agent stream apply (S0.1). */
export function shouldDispatchPanelForThread(originThreadId: string | undefined): boolean {
  if (!originThreadId) return true;
  if (activePanelThreadId === null) return true;
  return originThreadId === activePanelThreadId;
}

export function dispatchPanelScratchpad(
  scratchpad: ScratchpadStatus | null,
  originThreadId?: string,
): void {
  if (!shouldDispatchPanelForThread(originThreadId)) return;
  window.dispatchEvent(
    new CustomEvent(PANEL_SCRATCHPAD_EVENT, { detail: scratchpad }),
  );
}

export function dispatchPanelChecklist(
  checklist: ChecklistPanelPayload | null,
  originThreadId?: string,
): void {
  if (!shouldDispatchPanelForThread(originThreadId)) return;
  window.dispatchEvent(
    new CustomEvent(PANEL_CHECKLIST_EVENT, { detail: checklist }),
  );
}

export function dispatchPanelContext(
  context: ThreadContextSnapshot,
  originThreadId?: string,
): void {
  if (!shouldDispatchPanelForThread(originThreadId)) return;
  window.dispatchEvent(
    new CustomEvent(PANEL_CONTEXT_EVENT, { detail: context }),
  );
}

export function dispatchPanelContextUsage(
  usage: ContextUsageBreakdown,
  originThreadId?: string,
): void {
  if (!shouldDispatchPanelForThread(originThreadId)) return;
  window.dispatchEvent(
    new CustomEvent(PANEL_CONTEXT_USAGE_EVENT, { detail: usage }),
  );
}

export function dispatchPanelTaskGraph(
  task_graph: HarnessTaskGraph,
  originThreadId?: string,
): void {
  if (!shouldDispatchPanelForThread(originThreadId)) return;
  window.dispatchEvent(
    new CustomEvent(PANEL_TASK_GRAPH_EVENT, { detail: { task_graph } }),
  );
}

export function dispatchHarnessCycleAdvanced(
  detail: {
    from: number;
    to: number;
  },
  originThreadId?: string,
): void {
  if (!shouldDispatchPanelForThread(originThreadId)) return;
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
