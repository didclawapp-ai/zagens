/** Task record from GET /v1/tasks/:id (detail) */
export interface TaskRecord {
  schema_version: number;
  id: string;
  prompt: string;
  model: string;
  workspace: string;
  mode: string;
  allow_shell: boolean;
  trust_mode: boolean;
  auto_approve: boolean;
  status: TaskStatus;
  created_at: string;
  started_at: string | null;
  ended_at: string | null;
  duration_ms: number | null;
  result_summary: string | null;
  error: string | null;
  thread_id: string | null;
}

/** Lightweight task summary from GET /v1/tasks (list) */
export interface TaskSummary {
  id: string;
  status: TaskStatus;
  prompt_summary: string;
  model: string;
  mode: string;
  created_at: string;
  started_at: string | null;
  ended_at: string | null;
  duration_ms: number | null;
  error: string | null;
  thread_id: string | null;
  turn_id: string | null;
}

export interface TaskCounts {
  total: number;
  pending: number;
  running: number;
  completed: number;
  failed: number;
  canceled: number;
}

export interface TasksResponse {
  tasks: TaskSummary[];
  counts: TaskCounts;
}

export type TaskStatus =
  | 'pending'
  | 'running'
  | 'paused'
  | 'completed'
  | 'failed'
  | 'canceled';

/** Automation record from GET /v1/automations */
export type AutomationStatus =
  | 'active'
  | 'paused'
  | 'completed'
  | 'failed'
  | 'canceled';

export interface AutomationRecord {
  schema_version: number;
  id: string;
  name: string;
  prompt: string;
  rrule: string;
  cwds: string[];
  status: AutomationStatus;
  created_at: string;
  updated_at: string;
  next_run_at: string | null;
  last_run_at: string | null;
}

/** Skill entry from GET /v1/skills */
export interface SkillEntry {
  name: string;
  description: string;
  path: string;
}
