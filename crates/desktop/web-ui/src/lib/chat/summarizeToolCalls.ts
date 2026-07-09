import type { ToolCardModel } from '../../components/ToolCard';
import { parseFileNameFromToolInput } from '../diff/diffEntries';
import {
  toolCategory,
  type ToolCategory,
} from './timeline/toolCategories';

const DETAIL_MAX_CHARS = 42;

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
    case 'plan':
      return t('message.toolGroupPlan', { count: n });
    case 'office':
      return t('message.toolGroupOffice', { count: n });
    case 'workflow':
      return t('message.toolGroupWorkflow', { count: n });
    case 'agent':
      return t('message.toolGroupAgent', { count: n });
    default:
      return t('message.toolCallsDefault');
  }
}

function basenamePath(path: string): string {
  const parts = path.replace(/\\/g, '/').split('/');
  return parts[parts.length - 1] || path;
}

function tryParseShellCommand(input: string): string | null {
  try {
    const parsed = JSON.parse(input) as { command?: string };
    return typeof parsed.command === 'string' && parsed.command.trim()
      ? parsed.command.trim()
      : null;
  } catch {
    return null;
  }
}

function truncateDetail(text: string): string {
  const oneLine = text.replace(/\s+/g, ' ').trim();
  if (oneLine.length <= DETAIL_MAX_CHARS) return oneLine;
  return `${oneLine.slice(0, DETAIL_MAX_CHARS - 1).trimEnd()}…`;
}

function tryParseNamedField(input: string, field: string): string | null {
  try {
    const parsed = JSON.parse(input) as Record<string, unknown>;
    const value = parsed[field];
    return typeof value === 'string' && value.trim() ? value.trim() : null;
  } catch {
    return null;
  }
}

/**
 * Last tool's file/command snippet for an activity row (chronological end).
 */
export function lastToolActivityDetail(tools: ToolCardModel[]): string | null {
  if (tools.length === 0) return null;

  for (let i = tools.length - 1; i >= 0; i--) {
    const tool = tools[i];
    const cat = toolCategory(tool.name);
    if (cat === 'shell') {
      const cmd = tryParseShellCommand(tool.input) ?? tool.input.trim();
      if (cmd) return truncateDetail(cmd);
      continue;
    }
    if (cat === 'workflow') {
      const skill = tryParseNamedField(tool.input, 'name');
      if (skill) return truncateDetail(skill);
      const template = tryParseNamedField(tool.input, 'template');
      if (template) return truncateDetail(template);
      return truncateDetail(tool.name);
    }
    if (cat === 'agent') {
      const agentId = tryParseNamedField(tool.input, 'agent_id');
      if (agentId) return truncateDetail(agentId);
      const type =
        tryParseNamedField(tool.input, 'type') ??
        tryParseNamedField(tool.input, 'agent_type') ??
        tryParseNamedField(tool.input, 'role');
      if (type) return truncateDetail(type);
      return truncateDetail(tool.name);
    }
    if (cat === 'write' || cat === 'explore' || cat === 'office') {
      const path = parseFileNameFromToolInput(tool.input);
      if (path) return truncateDetail(basenamePath(path));
      continue;
    }
    const path = parseFileNameFromToolInput(tool.input);
    if (path) return truncateDetail(basenamePath(path));
    const cmd = tryParseShellCommand(tool.input);
    if (cmd) return truncateDetail(cmd);
  }
  return null;
}

/** Mixed-category activity summary, e.g. "写入 5 · 执行 3" (P4.6). */
export function summarizeActivityByCategory(
  tools: ToolCardModel[],
  t: (key: string, params?: Record<string, string>) => string,
): string {
  if (tools.length === 0) return t('message.toolCallsDefault');
  const order: ToolCategory[] = [
    'write',
    'office',
    'shell',
    'explore',
    'plan',
    'workflow',
    'agent',
    'other',
  ];
  const counts = new Map<ToolCategory, number>();
  for (const tool of tools) {
    const cat = toolCategory(tool.name);
    counts.set(cat, (counts.get(cat) ?? 0) + 1);
  }
  const parts: string[] = [];
  for (const cat of order) {
    const n = counts.get(cat);
    if (!n) continue;
    parts.push(groupLabel(cat, n, t));
  }
  if (parts.length === 0) return t('message.toolCallsDefault');
  if (parts.length === 1) return parts[0];
  return parts.join(' · ');
}

function withLastDetail(summary: string, tools: ToolCardModel[]): string {
  const detail = lastToolActivityDetail(tools);
  if (!detail) return summary;
  return `${summary} · ${detail}`;
}

/** Collapsed tools header — counts + last file/command for scanability. */
export function summarizeToolCalls(
  tools: ToolCardModel[],
  t: (key: string, params?: Record<string, string>) => string,
): string {
  if (tools.length === 0) {
    return t('message.toolCallsDefault');
  }

  const uniform = uniformCategory(tools);
  const summary =
    uniform !== 'mixed'
      ? groupLabel(uniform, tools.length, t)
      : summarizeActivityByCategory(tools, t);

  return withLastDetail(summary, tools);
}
