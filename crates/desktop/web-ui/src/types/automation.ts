export type TaskStatus = 'queued' | 'running' | 'completed' | 'failed' | 'canceled';

export function isTerminalTaskStatus(status: TaskStatus): boolean {
  return status === 'completed' || status === 'failed' || status === 'canceled';
}

export interface TaskTimelineEntry {
  timestamp: string;
  kind: string;
  summary: string;
  detail_path?: string | null;
}

export interface TaskToolCallSummary {
  id: string;
  name: string;
  status: string;
  started_at: string;
  ended_at?: string | null;
  duration_ms?: number | null;
  input_summary?: string | null;
  output_summary?: string | null;
}

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
  result_detail_path?: string | null;
  error: string | null;
  thread_id: string | null;
  turn_id?: string | null;
  timeline?: TaskTimelineEntry[];
  tool_calls?: TaskToolCallSummary[];
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

/** Matches runtime `task_manager::TaskCounts` (GET /v1/tasks). */
export interface TaskCounts {
  queued: number;
  running: number;
  completed: number;
  failed: number;
  canceled: number;
}

export interface TasksResponse {
  tasks: TaskSummary[];
  counts: TaskCounts;
}

/** Automation record from GET /v1/automations */
export type AutomationStatus = 'active' | 'paused';

export type AutomationTriggerKind =
  | 'prompt'
  | 'task'
  | 'night_queue_enqueue'
  | 'night_queue_run';

export function parseAutomationTriggerKind(raw?: string | null): AutomationTriggerKind {
  if (raw === 'task') return 'task';
  if (raw === 'night_queue_enqueue') return 'night_queue_enqueue';
  if (raw === 'night_queue_run') return 'night_queue_run';
  return 'prompt';
}

export interface AutomationRecord {
  schema_version: number;
  id: string;
  name: string;
  prompt: string;
  rrule: string;
  cwds: string[];
  trigger_kind?: AutomationTriggerKind;
  model?: string | null;
  mode?: string | null;
  allow_shell?: boolean | null;
  trust_mode?: boolean | null;
  auto_approve?: boolean | null;
  gate_preset?: string | null;
  gate?: string[];
  use_worktree?: boolean | null;
  write_briefing?: boolean | null;
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

/** Full GET /v1/skills payload */
export interface SkillsApiResponse {
  directory: string;
  warnings: string[];
  skills: SkillEntry[];
}

/** Body for POST /v1/skills */
export interface CreateSkillRequest {
  name: string;
  /** Default `workspace` when omitted. Ignored when `parent_directory` is set. */
  scope?: 'global' | 'workspace';
  /** Absolute path to an existing allowed skills root (desktop folder picker). */
  parent_directory?: string;
}

/** Response for POST /v1/skills (201 Created) */
export interface CreateSkillResponse {
  skill: SkillEntry;
  directory: string;
  skills_root: string;
  warnings: string[];
}

/** Body for POST /v1/skills/import */
export interface ImportSkillLocalRequest {
  source_directory: string;
  scope?: 'global' | 'workspace';
  parent_directory?: string;
  replace?: boolean;
}

/** Body for POST /v1/skills/install */
export interface InstallSkillRemoteRequest {
  spec: string;
  scope?: 'global' | 'workspace';
  parent_directory?: string;
  replace?: boolean;
}

/** Body for POST /v1/tasks */
export interface CreateTaskRequest {
  prompt: string;
  model?: string;
  workspace?: string;
  mode?: string;
  allow_shell?: boolean;
  trust_mode?: boolean;
  auto_approve?: boolean;
}

/** Body for POST /v1/automations */
export interface CreateAutomationRequest {
  name: string;
  prompt: string;
  rrule: string;
  cwds?: string[];
  trigger_kind?: AutomationTriggerKind;
  model?: string;
  mode?: string;
  allow_shell?: boolean;
  trust_mode?: boolean;
  auto_approve?: boolean;
  gate_preset?: string;
  gate?: string[];
  use_worktree?: boolean;
  write_briefing?: boolean;
  status?: AutomationStatus;
}

/** Body for PATCH /v1/automations/:id */
export interface UpdateAutomationRequest {
  name?: string;
  prompt?: string;
  rrule?: string;
  cwds?: string[];
  trigger_kind?: AutomationTriggerKind;
  model?: string;
  mode?: string;
  allow_shell?: boolean;
  trust_mode?: boolean;
  auto_approve?: boolean;
  gate_preset?: string;
  gate?: string[];
  use_worktree?: boolean;
  write_briefing?: boolean;
  status?: AutomationStatus;
}

export type AutomationRunStatus =
  | 'queued'
  | 'running'
  | 'completed'
  | 'failed'
  | 'canceled';

export interface AutomationRunRecord {
  id: string;
  automation_id: string;
  scheduled_for: string;
  status: AutomationRunStatus;
  created_at: string;
  started_at: string | null;
  ended_at: string | null;
  task_id: string | null;
  thread_id: string | null;
  turn_id: string | null;
  error: string | null;
  night_queue_task_id?: string | null;
  result_summary?: string | null;
}
