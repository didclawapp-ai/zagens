/** Night queue wire types — GET/POST /v1/night-queue/* */

export type NightQueueTaskStatus =
  | 'pending'
  | 'running'
  | 'passed'
  | 'failed'
  | 'rolled_back'
  | 'canceled';

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
  canceled?: number;
}

export interface NightQueueBriefingResponse {
  markdown: string;
  handoff_path?: string | null;
}

export interface NightQueueMutateResponse {
  task: NightQueueTask;
}

export interface NightQueueClearFinishedResponse {
  removed: number;
}

export interface NightQueueStopResponse {
  stopped: boolean;
  reclaimed?: number;
}

export function isActiveNightQueueStatus(status: NightQueueTaskStatus): boolean {
  return status === 'pending' || status === 'running';
}

export function isTerminalNightQueueStatus(status: NightQueueTaskStatus): boolean {
  return (
    status === 'passed' ||
    status === 'failed' ||
    status === 'rolled_back' ||
    status === 'canceled'
  );
}

export function shortNightQueueId(id: string): string {
  if (id.length <= 12) return id;
  return `${id.slice(0, 8)}…`;
}

export function formatNightQueueDuration(
  startedAt?: string | null,
  finishedAt?: string | null,
  nowMs: number = Date.now(),
  /** When true, omit duration unless finished_at is known (avoids "286h" on stale running). */
  requireFinished = false,
): string | null {
  if (!startedAt) return null;
  const start = Date.parse(startedAt);
  if (Number.isNaN(start)) return null;
  if (requireFinished && !finishedAt) return null;
  const end = finishedAt ? Date.parse(finishedAt) : nowMs;
  if (Number.isNaN(end) || end < start) return null;
  const sec = Math.max(0, Math.round((end - start) / 1000));
  if (sec < 60) return `${sec}s`;
  const min = Math.floor(sec / 60);
  const rem = sec % 60;
  if (min < 60) return `${min}m ${rem}s`;
  const hr = Math.floor(min / 60);
  return `${hr}h ${min % 60}m`;
}
