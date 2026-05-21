import type { ToolCardModel } from '../components/ToolCard';

export function formatToolForCopy(tool: ToolCardModel): string {
  const parts: string[] = [`# ${tool.name} (${tool.id})`, `status: ${tool.status}`];
  if (tool.input?.trim()) {
    parts.push('', '## input', tool.input.trim());
  }
  if (tool.output != null && String(tool.output).trim() !== '') {
    parts.push('', '## output', String(tool.output).trim());
  }
  return parts.join('\n');
}

export function formatToolsForCopy(tools: ToolCardModel[]): string {
  if (tools.length === 0) {
    return '';
  }
  return tools.map(formatToolForCopy).join('\n\n---\n\n');
}
