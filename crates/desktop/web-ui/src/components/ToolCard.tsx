import CopyTextButton from './CopyTextButton';
import { formatToolForCopy } from '../lib/formatToolCopy';
import { useT } from '../i18n';

export interface ToolCardModel {
  id: string;
  name: string;
  input: string;
  output?: string;
  status: 'running' | 'done' | 'error';
}

export function ToolCard({ tool, copyTitle }: { tool: ToolCardModel; copyTitle?: string }) {
  const { t } = useT();
  const copyLabel = copyTitle ?? t('chatMarkdown.copyTool');
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
    <div
      className="rounded-md border border-card-border/70 bg-transparent p-2.5 text-xs"
      role="region"
      aria-label={t('a11y.toolRegion', { name: tool.name, status: tool.status })}
      aria-busy={tool.status === 'running'}
    >
      <div className="mb-1 flex flex-wrap items-center gap-2">
        <span className="font-semibold text-t-text">{tool.name}</span>
        <span className="font-mono text-[11px] text-t-text-muted">{tool.id.slice(0, 12)}</span>
        <CopyTextButton
          getText={() => formatToolForCopy(tool)}
          title={copyLabel}
          disabled={!tool.input?.trim() && !(tool.output != null && String(tool.output).trim() !== '')}
          className="ml-auto"
        />
        <span className={`rounded border px-1.5 py-0.5 text-[11px] font-medium ${statusColor} ${statusBg}`}>
          {tool.status}
        </span>
      </div>
      {tool.input ? (
        <pre className="mt-1 max-h-28 overflow-auto whitespace-pre-wrap break-words leading-relaxed text-t-text-secondary">
          {tool.input}
        </pre>
      ) : null}
      {tool.output != null && tool.output !== '' ? (
        <pre className="mt-1.5 max-h-36 overflow-y-auto whitespace-pre-wrap break-words border-t border-divider pt-1.5 leading-relaxed text-t-text">
          {tool.output}
        </pre>
      ) : null}
    </div>
  );
}
