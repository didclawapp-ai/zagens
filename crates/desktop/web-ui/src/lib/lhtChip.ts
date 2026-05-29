/** Composer footer chip state from runtime `long_horizon.*` status events. */

export type LhtChipKind = 'continue' | 'blocked' | 'warning';

export interface LhtChipState {
  kind: LhtChipKind;
  /** Nudge count, open items, or context pressure %. */
  detail?: string;
  /** Machine reason from `long_horizon.blocked` status JSON. */
  reason?: string;
}

export function parseLhtStatusMessage(message: string): LhtChipState | null {
  const trimmed = message.trim();
  if (trimmed.startsWith('long_horizon.continue_injected:')) {
    const json = trimmed.slice('long_horizon.continue_injected:'.length).trim();
    try {
      const o = JSON.parse(json) as { nudge_count?: number; open_items?: number };
      const n = o.nudge_count ?? 0;
      const open = o.open_items ?? 0;
      return { kind: 'continue', detail: `${n}/${open}` };
    } catch {
      return { kind: 'continue' };
    }
  }
  if (trimmed.startsWith('long_horizon.blocked:')) {
    const json = trimmed.slice('long_horizon.blocked:'.length).trim();
    try {
      const o = JSON.parse(json) as { open_items?: number; reason?: string };
      return {
        kind: 'blocked',
        detail: String(o.open_items ?? ''),
        reason: typeof o.reason === 'string' ? o.reason : undefined,
      };
    } catch {
      return { kind: 'blocked' };
    }
  }
  if (trimmed.startsWith('long_horizon.context_warning:')) {
    const json = trimmed.slice('long_horizon.context_warning:'.length).trim();
    try {
      const o = JSON.parse(json) as { pressure_pct?: number };
      return { kind: 'warning', detail: String(o.pressure_pct ?? '') };
    } catch {
      return { kind: 'warning' };
    }
  }
  return null;
}

/** Merge task-graph API hints when SSE status was missed. */
export function lhtChipFromTaskGraph(
  graph: {
    lht_enabled?: boolean;
    incomplete?: boolean;
    lht_blocked?: boolean | null;
    nudge_count?: number | null;
    open_items?: number;
  } | null,
): LhtChipState | null {
  if (!graph?.lht_enabled || !graph.incomplete) {
    return null;
  }
  if (graph.lht_blocked) {
    return {
      kind: 'blocked',
      detail: String(graph.open_items ?? ''),
      reason: 'max_nudges_without_progress',
    };
  }
  const n = graph.nudge_count ?? 0;
  if (n > 0) {
    return { kind: 'continue', detail: `${n}/${graph.open_items ?? 0}` };
  }
  return null;
}
