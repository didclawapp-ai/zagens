export type ToolCategory =
  | 'explore'
  | 'write'
  | 'shell'
  | 'plan'
  | 'office'
  | 'workflow'
  | 'agent'
  | 'other';

const EXPLORE_TOOLS = new Set([
  'read_file',
  'glob_file_search',
  'glob_files',
  'grep',
  'grep_files',
  'list_dir',
  'codebase_search',
  'semantic_search',
  'search_files',
  'file_search',
  'web_search',
  'web.run',
  'diagnostics',
]);

const WRITE_TOOLS = new Set([
  'write_file',
  'edit_file',
  'apply_patch',
  'str_replace',
  'str_replace_editor',
]);

const SHELL_TOOLS = new Set([
  'exec_shell',
  'exec_shell_wait',
  'exec_shell_interact',
  'exec_wait',
  'exec_interact',
  'task_shell_start',
  'task_shell_wait',
  'run_terminal_cmd',
]);

const PLAN_TOOLS = new Set([
  'update_plan',
  'checklist_write',
  'checklist_update',
  'todo_write',
  'todo_update',
]);

/** Office document tools — collapse like explore/write in activity bundles. */
const OFFICE_TOOLS = new Set([
  'read_office',
  'write_office',
  'load_office_payload',
]);

/**
 * Setup / audit workflow tools (skills + scratchpad).
 * Shown expanded in screenshots when left as `other` — include in bundling.
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
  // Deferred-tool discovery (shown expanded between scratchpad/agent when left as other).
  'tool_search_tool_regex',
  'tool_search_tool_bm25',
  'tool_search_bm25',
]);

/** Sub-agent orchestration — prompts are huge; must collapse in the timeline. */
const AGENT_TOOLS = new Set([
  'agent_spawn',
  'spawn_agent',
  'delegate_to_agent',
  'agent_wait',
  'agent_result',
  'agent_cancel',
  'close_agent',
  'agent_list',
  'agent_assign',
  'agent_send_input',
  'send_input',
  'resume_agent',
]);

export function toolCategory(name: string): ToolCategory {
  if (EXPLORE_TOOLS.has(name)) return 'explore';
  if (WRITE_TOOLS.has(name)) return 'write';
  if (SHELL_TOOLS.has(name)) return 'shell';
  if (PLAN_TOOLS.has(name)) return 'plan';
  if (OFFICE_TOOLS.has(name)) return 'office';
  if (WORKFLOW_TOOLS.has(name)) return 'workflow';
  if (AGENT_TOOLS.has(name)) return 'agent';
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

/** Categories that participate in activity bundling / compact rows. */
export function isCollapsibleToolCategory(category: ToolCategory): boolean {
  return (
    category === 'explore' ||
    category === 'write' ||
    category === 'shell' ||
    category === 'plan' ||
    category === 'office' ||
    category === 'workflow' ||
    category === 'agent'
  );
}
