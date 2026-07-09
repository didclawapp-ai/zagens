export type ToolCategory = 'explore' | 'write' | 'shell' | 'plan' | 'other';

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

export function toolCategory(name: string): ToolCategory {
  if (EXPLORE_TOOLS.has(name)) return 'explore';
  if (WRITE_TOOLS.has(name)) return 'write';
  if (SHELL_TOOLS.has(name)) return 'shell';
  if (PLAN_TOOLS.has(name)) return 'plan';
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
