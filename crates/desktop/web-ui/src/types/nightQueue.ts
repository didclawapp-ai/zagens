/** Night queue wire types — GET/POST /v1/night-queue/* */

export type NightQueueTaskStatus =
  | 'pending'
  | 'running'
  | 'passed'
  | 'failed'
  | 'rolled_back';

export interface GatePredicateWire {
  predicate: string;
  args?: Record<string, unknown>;
}

export interface NightQueueTask {
  id: string;
  prompt: string;
  status: NightQueueTaskStatus;
  worktree_path?: string | null;
  gate: GatePredicateWire[];
  created_at: string;
  started_at?: string | null;
  finished_at?: string | null;
  pre_snapshot_id?: string | null;
  gate_summary?: string | null;
  error?: string | null;
}

export interface NightQueueResponse {
  schema_version: number;
  last_run_at?: string | null;
  tasks: NightQueueTask[];
  queue_path: string;
}

export interface GatePreset {
  id: string;
  description: string;
}

export interface GatePresetsResponse {
  presets: GatePreset[];
}

export interface CreateNightQueueTaskRequest {
  prompt: string;
  gate?: string[];
  gate_file?: string;
  gate_preset?: string;
  use_worktree?: boolean;
}

export interface RunNightQueueRequest {
  max_parallel?: number;
  use_worktree?: boolean;
  write_briefing?: boolean;
}

export interface RunNightQueueResponse {
  ran: number;
  passed: number;
  failed: number;
}

export interface NightQueueBriefingResponse {
  markdown: string;
  handoff_path?: string | null;
}

export function isActiveNightQueueStatus(status: NightQueueTaskStatus): boolean {
  return status === 'pending' || status === 'running';
}
