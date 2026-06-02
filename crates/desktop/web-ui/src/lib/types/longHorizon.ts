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

/** Composable harness completion gate live counters (P2). */
export interface HarnessCompletionGate {
  active: boolean;
  mode?: string | null;
  /** Generic layer-2: model `[verify:]` replay mode (off/observe/enforce). */
  auto_verify_replay?: string | null;
  /** Generic layer-2: toolchain build/test gate mode (off/observe/enforce). */
  toolchain_gate?: string | null;
  manifest_round: number;
  audit_round: number;
  first_gap_count?: number | null;
  /** Cross-layer integration gate gap count (P1′). */
  integration_gap_count?: number | null;
  gate_reinject_while_blocked: number;
  last_manifest_passed?: boolean | null;
  last_audit_pass?: boolean | null;
  last_unmet_reason?: string | null;
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
  /** Completion gate summary (manifest / audit rounds, P2). */
  completion_gate?: HarnessCompletionGate | null;
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
