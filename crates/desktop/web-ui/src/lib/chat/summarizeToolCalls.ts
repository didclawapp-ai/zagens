import type { ToolCardModel } from '../../components/ToolCard';

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

type ToolCategory = 'explore' | 'write' | 'shell' | 'other';

function toolCategory(name: string): ToolCategory {
  if (EXPLORE_TOOLS.has(name)) {
    return 'explore';
  }
  if (WRITE_TOOLS.has(name)) {
    return 'write';
  }
  if (SHELL_TOOLS.has(name)) {
    return 'shell';
  }
  return 'other';
}

function uniformCategory(tools: ToolCardModel[]): ToolCategory | 'mixed' {
  if (tools.length === 0) {
    return 'mixed';
  }
  const first = toolCategory(tools[0].name);
  for (let i = 1; i < tools.length; i++) {
    if (toolCategory(tools[i].name) !== first) {
      return 'mixed';
    }
  }
  return first;
}

function groupLabel(
  category: ToolCategory,
  count: number,
  t: (key: string, params?: Record<string, string>) => string,
): string {
  const n = String(count);
  switch (category) {
    case 'explore':
      return t('message.toolGroupReads', { count: n });
    case 'write':
      return t('message.toolGroupWrites', { count: n });
    case 'shell':
      return t('message.toolGroupShell', { count: n });
    default:
      return t('message.toolCallsDefault');
  }
}

/** Collapsed tools header — aggregate repetitive calls like premium agent UIs. */
export function summarizeToolCalls(
  tools: ToolCardModel[],
  t: (key: string, params?: Record<string, string>) => string,
): string {
  if (tools.length === 0) {
    return t('message.toolCallsDefault');
  }

  const uniform = uniformCategory(tools);
  if (uniform !== 'mixed') {
    return groupLabel(uniform, tools.length, t);
  }

  const uniqueNames = [...new Set(tools.map((tool) => tool.name))];
  if (uniqueNames.length === 1) {
    return tools.length === 1 ? uniqueNames[0] : `${uniqueNames[0]} ×${tools.length}`;
  }

  const head = uniqueNames.slice(0, 2).join(' · ');
  if (uniqueNames.length > 2 || tools.length > 2) {
    return t('message.toolCallsHeadMore', { head, count: String(tools.length) });
  }
  return head;
}
