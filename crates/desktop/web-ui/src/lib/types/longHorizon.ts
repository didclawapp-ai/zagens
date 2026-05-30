export interface TaskGraphPhase {
  step: string;
  status: 'pending' | 'in_progress' | 'completed';
}

export interface TaskGraphChecklistItem {
  id: number;
  content: string;
  verify_command?: string | null;
  status: 'pending' | 'in_progress' | 'completed';
}

export interface HarnessNudgeTelemetry {
  emitted: number;
  converted: number;
  blocked: number;
  conversion_pct: number;
}

/** One harness node decision for the LHT "nodes" tab (DEMO5 #3). */
export interface HarnessNode {
  /** e.g. 'continue_injected', 'gate_skip', 'incomplete_stop', 'verify_gate'. */
  kind: string;
  /** Epoch milliseconds when the decision was recorded. */
  ts_ms: number;
  /** Parsed `{…}` payload (reason / open_items / nudge_count / verdict / …). */
  payload?: Record<string, unknown> | null;
}

export interface HarnessTaskGraph {
  objective: string;
  objective_source?: string;
  phases: TaskGraphPhase[];
  checklist: TaskGraphChecklistItem[];
  completion_pct: number;
  open_items: number;
  in_progress_id?: number | null;
  incomplete: boolean;
  lht_enabled: boolean;
  lht_blocked?: boolean | null;
  nudge_count?: number | null;
  /** Nudge effectiveness telemetry (§4.9) — present when an engine is live. */
  telemetry?: HarnessNudgeTelemetry | null;
  /** Recent harness node-decision trail (newest last); DEMO5 #3. */
  recent_nodes?: HarnessNode[] | null;
}

export interface HarnessCycleBriefing {
  cycle: number;
  timestamp: string;
  briefing_preview: string;
  token_estimate: number;
}

export interface HarnessCycleArchive {
  cycle: number;
  started: string;
  ended: string;
  message_count: number;
}

export interface HarnessCycles {
  cycle_count: number;
  current_cycle: number;
  briefings: HarnessCycleBriefing[];
  archives?: HarnessCycleArchive[];
  context_pressure_pct?: number | null;
  context_window_tokens?: number | null;
  cycle_threshold_tokens?: number | null;
  lht_warning_low_pct?: number | null;
  lht_warning_high_pct?: number | null;
}

export type LongHorizonPanelTab = 'task' | 'cycle' | 'context' | 'nodes';
