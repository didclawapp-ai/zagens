export type ToolCategory =
  | 'explore'
  | 'write'
  | 'shell'
  | 'plan'
  | 'office'
  | 'workflow'
  | 'agent'
  | 'browser'
  | 'other';

/**
 * Read / search / inspect — high-churn in long turns; must collapse.
 * Keep in sync with runtime-server tool registry names (+ common aliases).
 */
const EXPLORE_TOOLS = new Set([
  'read_file',
  'file_info',
  'list_dir',
  'glob_file_search',
  'glob_files',
  'grep',
  'grep_files',
  'codebase_search',
  'semantic_search',
  'search_files',
  'file_search',
  'explore_codebase',
  'investigate',
  'answer_from_repo',
  'project_map',
  'promote_to_context',
  'describe_image',
  'web_search',
  'web.run',
  'fetch_url',
  'finance',
  'diagnostics',
  'validate_data',
  'review',
  'git_status',
  'git_diff',
  'git_log',
  'git_show',
  'git_blame',
  'github_issue_context',
  'github_pr_context',
]);

const WRITE_TOOLS = new Set([
  'write_file',
  'edit_file',
  'apply_patch',
  'batch_edit',
  'refactor_imports',
  'restore_file',
  'fim_edit',
  'edit_and_check',
  'change_and_verify',
  'str_replace',
  'str_replace_editor',
  'revert_turn',
]);

const SHELL_TOOLS = new Set([
  'exec_shell',
  'exec_shell_wait',
  'exec_shell_interact',
  'exec_shell_cancel',
  'exec_wait',
  'exec_interact',
  'task_shell_start',
  'task_shell_wait',
  'run_terminal_cmd',
  'run_tests',
  'code_execution',
]);

const PLAN_TOOLS = new Set([
  'update_plan',
  'checklist_write',
  'checklist_add',
  'checklist_update',
  'checklist_list',
  'todo_write',
  'todo_add',
  'todo_update',
  'todo_list',
]);

/** Office document tools — collapse like explore/write in activity bundles. */
const OFFICE_TOOLS = new Set([
  'read_office',
  'write_office',
  'load_office_payload',
]);

/**
 * Setup / audit / harness / durable-task workflow tools.
 * Shown expanded when left as `other` — include in bundling.
 */
const WORKFLOW_TOOLS = new Set([
  'load_skill',
  'draft_skill',
  'scratchpad_init',
  'scratchpad_status',
  'scratchpad_append',
  'scratchpad_list_notes',
  'scratchpad_set_area',
  'scratchpad_verify_note',
  'scratchpad_import_agent',
  'tool_search_tool_regex',
  'tool_search_tool_bm25',
  'tool_search_bm25',
  'note',
  'remember',
  'recall_archive',
  'rlm',
  'assert_file_count',
  'assert_output_matches',
  'assert_tests_pass',
  'task_create',
  'task_list',
  'task_read',
  'task_cancel',
  'task_gate_run',
  'pr_attempt_record',
  'pr_attempt_list',
  'pr_attempt_read',
  'pr_attempt_preflight',
  'automation_create',
  'automation_list',
  'automation_read',
  'automation_update',
  'automation_pause',
  'automation_resume',
  'automation_delete',
  'automation_run',
  'github_comment',
  'github_close_issue',
]);

/** Sub-agent orchestration — prompts are huge; must collapse in the timeline. */
const AGENT_TOOLS = new Set([
  'agent_spawn',
  'spawn_agent',
  'delegate_to_agent',
  'agent_wait',
  'wait',
  'agent_result',
  'agent_cancel',
  'close_agent',
  'agent_list',
  'agent_assign',
  'assign_agent',
  'agent_send_input',
  'send_input',
  'resume_agent',
]);

/** Built-in Browser pane tools — high churn in browse turns; must collapse. */
const BROWSER_TOOLS = new Set([
  'browser_navigate',
  'browser_snapshot',
  'browser_get_text',
  'browser_console_tail',
  'browser_click',
  'browser_type',
  'browser_scroll',
  'browser_wait',
  'browser_start_preview',
]);

/**
 * Intentionally left as `other` (stay expanded / rare meta):
 * - `request_user_input` — interactive approval UI
 * - `multi_tool_use.parallel` — batch meta wrapper
 */

export function toolCategory(name: string): ToolCategory {
  if (EXPLORE_TOOLS.has(name)) return 'explore';
  if (WRITE_TOOLS.has(name)) return 'write';
  if (SHELL_TOOLS.has(name)) return 'shell';
  if (PLAN_TOOLS.has(name)) return 'plan';
  if (OFFICE_TOOLS.has(name)) return 'office';
  if (WORKFLOW_TOOLS.has(name)) return 'workflow';
  if (AGENT_TOOLS.has(name)) return 'agent';
  if (BROWSER_TOOLS.has(name)) return 'browser';
  return 'other';
}

export function isExploreTool(name: string): boolean {
  return toolCategory(name) === 'explore';
}

export function isWriteTool(name: string): boolean {
  return toolCategory(name) === 'write';
}

export function isPlanTool(name: string): boolean {
  return toolCategory(name) === 'plan';
}

export function isOfficeTool(name: string): boolean {
  return toolCategory(name) === 'office';
}

export function isWorkflowTool(name: string): boolean {
  return toolCategory(name) === 'workflow';
}

export function isAgentTool(name: string): boolean {
  return toolCategory(name) === 'agent';
}

export function isBrowserTool(name: string): boolean {
  return toolCategory(name) === 'browser';
}

/** Categories that participate in activity bundling / compact rows. */
export function isCollapsibleToolCategory(category: ToolCategory): boolean {
  return (
    category === 'explore' ||
    category === 'write' ||
    category === 'shell' ||
    category === 'plan' ||
    category === 'office' ||
    category === 'workflow' ||
    category === 'agent' ||
    category === 'browser'
  );
}
