export interface ToolCardModel {
  id: string;
  name: string;
  input: string;
  output?: string;
  status: 'running' | 'done' | 'error';
}

export function ToolCard({ tool }: { tool: ToolCardModel }) {
  const statusColor =
    tool.status === 'running'
      ? 'text-amber-300'
      : tool.status === 'error'
        ? 'text-red-400'
        : 'text-emerald-400';

  return (
    <div className="mt-2 rounded-lg border border-gray-600/50 bg-gray-900/50 p-2 text-xs">
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-semibold text-amber-200/90">{tool.name}</span>
        <span className="text-gray-500 font-mono">{tool.id.slice(0, 12)}</span>
        <span className={`${statusColor} ml-auto`}>{tool.status}</span>
      </div>
      {tool.input ? (
        <pre className="mt-1 max-h-28 overflow-auto text-gray-400 whitespace-pre-wrap break-words">
          {tool.input}
        </pre>
      ) : null}
      {tool.output != null && tool.output !== '' ? (
        <pre className="mt-1 max-h-36 overflow-y-auto text-gray-300 whitespace-pre-wrap break-words border-t border-gray-700/50 pt-1">
          {tool.output}
        </pre>
      ) : null}
    </div>
  );
}
