import { useMemo, useState } from 'react';
import { useT } from '../i18n';
import type {
  McpCallRecord,
  McpDiscoveredItem,
  McpServerDiscoverEntry,
  McpServerEntry,
  McpToolEntry,
} from '../types/mcp';

type DetailTab = 'tools' | 'resources' | 'prompts' | 'calls';

export default function McpServerDetail({
  server,
  discover,
  tools,
  recentCalls,
  togglingTool,
  onToggleTool,
}: {
  server: McpServerEntry;
  discover?: McpServerDiscoverEntry;
  tools: McpToolEntry[];
  recentCalls: McpCallRecord[];
  togglingTool: string | null;
  onToggleTool: (toolName: string, enable: boolean) => void | Promise<void>;
}) {
  const { t } = useT();
  const [tab, setTab] = useState<DetailTab>('tools');

  const toolSchemaByName = useMemo(() => {
    const m = new Map<string, McpToolEntry>();
    for (const tool of tools) {
      if (tool.server === server.name) m.set(tool.name, tool);
    }
    return m;
  }, [tools, server.name]);

  const serverCalls = useMemo(
    () => recentCalls.filter((c) => c.server === server.name).slice(-20),
    [recentCalls, server.name],
  );

  const resources = discover?.resources ?? [];
  const prompts = discover?.prompts ?? [];
  const discoveredTools = discover?.tools ?? [];

  return (
    <div className="flex flex-col min-h-0 flex-1 overflow-hidden">
      <div className="px-3 py-2 border-b border-divider shrink-0 flex items-center gap-2 flex-wrap text-[11px]">
        <span
          className={`px-1.5 py-0.5 rounded font-medium ${
            server.connected ? 'bg-success-bg text-success' : 'bg-error-bg text-t-error'
          }`}
        >
          {server.connected ? t('mcp.connected') : t('mcp.disconnected')}
        </span>
        {discover?.error && (
          <span className="text-t-error truncate max-w-full" title={discover.error}>
            {discover.error}
          </span>
        )}
      </div>

      <div className="flex items-center gap-1 px-3 py-2 border-b border-divider shrink-0 overflow-x-auto">
        {(['tools', 'resources', 'prompts', 'calls'] as const).map((id) => (
          <button
            key={id}
            type="button"
            onClick={() => setTab(id)}
            className={`px-2.5 py-1 rounded text-xs font-medium whitespace-nowrap ${
              tab === id ? 'bg-accent-soft text-accent' : 'text-t-text-muted hover:bg-hover'
            }`}
          >
            {t(
              id === 'tools'
                ? 'mcp.tab.tools'
                : id === 'resources'
                  ? 'mcp.tab.resources'
                  : id === 'prompts'
                    ? 'mcp.tab.prompts'
                    : 'mcp.tab.calls',
              {
                count: String(
                  id === 'tools'
                    ? discoveredTools.length
                    : id === 'resources'
                      ? resources.length
                      : id === 'prompts'
                        ? prompts.length
                        : serverCalls.length,
                ),
              },
            )}
          </button>
        ))}
      </div>

      <div className="overflow-y-auto px-3 py-2 space-y-1.5 flex-1 min-h-0">
        {tab === 'tools' &&
          (discoveredTools.length === 0 ? (
            <p className="text-xs text-t-text-muted text-center py-6">{t('mcp.noTools')}</p>
          ) : (
            discoveredTools.map((tool) => (
              <ToolRowWithToggle
                key={tool.name}
                tool={tool}
                schema={toolSchemaByName.get(tool.name)}
                busy={togglingTool === tool.name}
                onToggle={(enable) => void onToggleTool(tool.name, enable)}
              />
            ))
          ))}

        {tab === 'resources' &&
          (resources.length === 0 ? (
            <p className="text-xs text-t-text-muted text-center py-6">{t('mcp.noResources')}</p>
          ) : (
            resources.map((item) => <DiscoverRow key={item.model_name} item={item} />)
          ))}

        {tab === 'prompts' &&
          (prompts.length === 0 ? (
            <p className="text-xs text-t-text-muted text-center py-6">{t('mcp.noPrompts')}</p>
          ) : (
            prompts.map((item) => <DiscoverRow key={item.model_name} item={item} />)
          ))}

        {tab === 'calls' &&
          (serverCalls.length === 0 ? (
            <p className="text-xs text-t-text-muted text-center py-6">{t('mcp.noCalls')}</p>
          ) : (
            serverCalls.map((call, idx) => (
              <CallRow key={`${call.timestamp_ms}-${idx}`} call={call} />
            ))
          ))}
      </div>
    </div>
  );
}

function DiscoverRow({ item }: { item: McpDiscoveredItem }) {
  return (
    <div className="rounded border border-divider bg-canvas-alt px-3 py-2">
      <div className="font-mono text-xs text-accent">{item.name}</div>
      {item.description && (
        <p className="text-[11px] text-t-text-muted mt-0.5">{item.description}</p>
      )}
      <p className="text-[10px] text-t-text-muted font-mono mt-0.5 opacity-70">{item.model_name}</p>
    </div>
  );
}

function ToolRowWithToggle({
  tool,
  schema,
  busy,
  onToggle,
}: {
  tool: McpDiscoveredItem;
  schema?: McpToolEntry;
  busy: boolean;
  onToggle: (enable: boolean) => void;
}) {
  const { t } = useT();
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="rounded border border-divider bg-canvas-alt overflow-hidden">
      <div className="flex items-center gap-2 px-3 py-2">
        <label className="flex items-center gap-1.5 shrink-0 cursor-pointer">
          <input
            type="checkbox"
            checked={tool.enabled}
            disabled={busy}
            onChange={(e) => onToggle(e.target.checked)}
            className="rounded border-input-border"
          />
          <span className="text-[10px] text-t-text-muted">{t('mcp.toolEnabled')}</span>
        </label>
        <button
          type="button"
          onClick={() => setExpanded(!expanded)}
          className="flex-1 min-w-0 text-left hover:bg-hover rounded px-1 py-0.5 flex items-center gap-2"
        >
          <span className="font-mono text-xs text-accent truncate">{tool.model_name}</span>
          {tool.description && (
            <span className="text-[11px] text-t-text-muted truncate">— {tool.description}</span>
          )}
        </button>
      </div>
      {expanded && schema && (
        <div className="border-t border-divider px-3 py-2">
          <pre className="text-[10px] font-mono text-t-text-muted whitespace-pre-wrap max-h-32 overflow-auto">
            {JSON.stringify(schema.input_schema, null, 2)}
          </pre>
        </div>
      )}
    </div>
  );
}

function CallRow({ call }: { call: McpCallRecord }) {
  const time = new Date(call.timestamp_ms).toLocaleTimeString();
  return (
    <div
      className={`rounded border px-3 py-2 text-[11px] ${
        call.success ? 'border-divider bg-canvas-alt' : 'border-t-error/30 bg-error-bg/20'
      }`}
    >
      <div className="flex justify-between gap-2">
        <span className="font-mono text-accent">{call.method}</span>
        <span className="text-t-text-muted shrink-0">
          {time} · {call.duration_ms}ms · {call.result_bytes}B
        </span>
      </div>
      {!call.success && call.error && (
        <p className="text-t-error mt-1 break-words">{call.error}</p>
      )}
    </div>
  );
}
