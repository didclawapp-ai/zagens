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
      ? 'text-amber border-amber/30'
      : tool.status === 'error'
        ? 'text-t-error border-t-error/30'
        : 'text-success border-success/30';

  const statusBg =
    tool.status === 'running'
      ? 'bg-amber-bg'
      : tool.status === 'error'
        ? 'bg-error-bg'
        : 'bg-success-bg';

  return (
    <div className="mt-2 rounded-lg border border-card-border bg-canvas-alt p-2.5 text-xs">
      <div className="flex flex-wrap items-center gap-2 mb-1">
        <span className="font-semibold text-t-text">{tool.name}</span>
        <span className="text-t-text-muted font-mono text-[11px]">{tool.id.slice(0, 12)}</span>
        <span className={`ml-auto px-1.5 py-0.5 rounded text-[11px] font-medium border ${statusColor} ${statusBg}`}>
          {tool.status}
        </span>
      </div>
      {tool.input ? (
        <pre className="mt-1 max-h-28 overflow-auto text-t-text-secondary whitespace-pre-wrap break-words leading-relaxed">
          {tool.input}
        </pre>
      ) : null}
      {tool.output != null && tool.output !== '' ? (
        <pre className="mt-1.5 max-h-36 overflow-y-auto text-t-text whitespace-pre-wrap break-words border-t border-divider pt-1.5 leading-relaxed">
          {tool.output}
        </pre>
      ) : null}
    </div>
  );
}
