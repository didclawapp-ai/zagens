export type TraceLane = 'model' | 'tools' | 'guards';

export type TraceTab = 'overview' | 'timeline' | 'turnmap' | 'memory' | 'harness' | 'replay';

export interface TraceEventEnvelope {
  seq: number;
  ts_ms: number;
  payload: Record<string, unknown>;
}

export interface TraceTurnSummary {
  turn_id: string;
  event_count: number;
  coherence_ok: boolean;
  coherence_error?: string | null;
  outcome?: string | null;
}

export interface TraceReplaySummary {
  coherence_ok: boolean;
  coherence_error?: string | null;
  turns: TraceTurnSummary[];
  effect_counts: Record<string, number>;
  synthetic_timeline: boolean;
}

export interface TraceCompactionEntry {
  turn_id: string;
  artifact_id: string;
  replaced_from: number;
  replaced_to: number;
}

export interface TraceCapacityEntry {
  turn_id: string;
  step_idx: number;
  tokens_used: number;
  token_budget: number;
  action: string;
}

export interface TraceAnalysis {
  compaction_timeline: TraceCompactionEntry[];
  capacity_checkpoints: TraceCapacityEntry[];
}

export interface TraceBundle {
  schema_version: number;
  generator: { tool: string; version: string; generated_at_ms: number };
  source: { kind: string; fixture_path?: string; thread_id?: string; workspace_label?: string };
  replay_summary: TraceReplaySummary;
  events: TraceEventEnvelope[];
  harness?: {
    task_graph?: Record<string, unknown>;
    nodes_source?: string;
    snapshot_source?: string;
  } | null;
  analysis?: TraceAnalysis | null;
  redaction: { applied: boolean; rules: string[] };
}

export interface TraceEffectCountDelta {
  field: string;
  left: number;
  right: number;
  delta: number;
}

export interface TraceGuardEventDelta {
  kind: string;
  left: number;
  right: number;
}

export interface TraceCompareDiff {
  coherence_match: boolean;
  left_coherence_ok: boolean;
  right_coherence_ok: boolean;
  event_kind_sequence_match: boolean;
  left_event_kinds: string[];
  right_event_kinds: string[];
  first_kind_mismatch_index?: number | null;
  left_event_count: number;
  right_event_count: number;
  turn_count_left: number;
  turn_count_right: number;
  effect_count_deltas: TraceEffectCountDelta[];
  guard_event_deltas: TraceGuardEventDelta[];
}

export interface TraceCompareSide {
  label: string;
  bundle: TraceBundle;
}

export interface TraceCompareDocument {
  document_kind: 'compare';
  schema_version: number;
  generator: { tool: string; version: string; generated_at_ms: number };
  left: TraceCompareSide;
  right: TraceCompareSide;
  diff: TraceCompareDiff;
}

export type TraceDocument = TraceBundle | TraceCompareDocument;

export type CompareTab = 'overview' | 'replay';

export interface LaneEvent {
  seq: number;
  ts_ms: number;
  kind: string;
  label: string;
  payload: Record<string, unknown>;
}

export interface LaneGroup {
  lane: TraceLane;
  title: string;
  events: LaneEvent[];
}
