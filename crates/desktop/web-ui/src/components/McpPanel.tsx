import { useEffect, useState } from 'react';
import {
  fetchMcpServers,
  fetchMcpTools,
  mergeMcpConfigJson,
  invalidateRuntimeBootReadyCache,
  type RuntimeConnectionState,
} from '../api/client';
import type { McpServerEntry, McpToolEntry } from '../types/mcp';

export default function McpPanel({ runtimeConn }: { runtimeConn: RuntimeConnectionState }) {
  const [servers, setServers] = useState<McpServerEntry[]>([]);
  const [tools, setTools] = useState<McpToolEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedServer, setSelectedServer] = useState<string | null>(null);
  const [showAddDialog, setShowAddDialog] = useState(false);
  const [showRestartDialog, setShowRestartDialog] = useState(false);
  const [restartPending, setRestartPending] = useState(false);

  const reload = async () => {
    setLoading(true);
    setError(null);
    try {
      const [srv, tl] = await Promise.all([
        fetchMcpServers(),
        fetchMcpTools(selectedServer ?? undefined),
      ]);
      setServers(srv.servers);
      setTools(tl.tools);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (runtimeConn === 'connected') {
      reload();
    }
  }, [runtimeConn, selectedServer]);

  const handleMergeMcpJson = async (jsonText: string) => {
    setError(null);
    try {
      await mergeMcpConfigJson(jsonText);
      setShowAddDialog(false);
      setShowRestartDialog(true);
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleRestartSidecar = async () => {
    setRestartPending(true);
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('restart_sidecar');
      invalidateRuntimeBootReadyCache();
    } catch {
      /* Best-effort — desktop invoke may throw in browser mode */
    } finally {
      setRestartPending(false);
      setShowRestartDialog(false);
    }
  };

  if (runtimeConn !== 'connected') {
    return (
      <div className="p-4 text-xs text-t-text-muted text-center space-y-2">
        <p>等待运行时连接…</p>
        <p className="text-[10px]">MCP 配置将在桌面运行时就绪后自动加载。</p>
      </div>
    );
  }

  if (loading && servers.length === 0 && !error) {
    return (
      <div className="p-4 text-xs text-t-text-muted text-center">
        <p>正在加载 MCP 配置…</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full overflow-hidden relative">
      {/* Header bar with add button */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-divider shrink-0">
        <span className="text-[11px] text-t-text-muted">
          {servers.length} 个服务器 · {tools.length} 个工具
        </span>
        <div className="flex-1" />
        <button
          type="button"
          onClick={() => setShowAddDialog(!showAddDialog)}
          className="px-2.5 py-1 rounded text-xs font-medium bg-accent text-accent-text hover:opacity-90 transition-opacity"
        >
          {showAddDialog ? '关闭' : '合并 MCP (JSON)'}
        </button>
      </div>

      {error && (
        <p className="shrink-0 px-3 py-1.5 text-[11px] text-t-error bg-error-bg/30 border-b border-divider">
          {error}
        </p>
      )}

      {/* Inline add dialog */}
      {showAddDialog && (
        <AddMcpJsonForm
          onSubmit={handleMergeMcpJson}
          onCancel={() => setShowAddDialog(false)}
        />
      )}

      {/* Server filter tabs */}
      <div className="flex items-center gap-1 px-3 py-2 overflow-x-auto border-b border-divider shrink-0">
        <button
          type="button"
          onClick={() => setSelectedServer(null)}
          className={`px-2.5 py-1 rounded text-xs font-medium transition-colors ${
            selectedServer === null
              ? 'bg-accent-soft text-accent'
              : 'text-t-text-muted hover:text-t-text hover:bg-hover'
          }`}
        >
          全部 ({tools.length})
        </button>
        {servers.map((s) => (
          <button
            key={s.name}
            type="button"
            onClick={() => setSelectedServer(s.name)}
            className={`px-2.5 py-1 rounded text-xs font-medium transition-colors whitespace-nowrap ${
              selectedServer === s.name
                ? 'bg-accent-soft text-accent'
                : 'text-t-text-muted hover:text-t-text hover:bg-hover'
            }`}
          >
            {s.name}
            <span className="ml-1 text-[10px] opacity-70">
              ({s.tool_count})
            </span>
          </button>
        ))}
      </div>

      {/* Server list (when no server filter) */}
      {selectedServer === null && (
        <div className="overflow-y-auto px-3 py-2 space-y-2">
          {servers.map((s) => (
            <ServerCard
              key={s.name}
              server={s}
              onSelect={() => setSelectedServer(s.name)}
            />
          ))}
          {servers.length === 0 && !loading && (
            <p className="text-xs text-t-text-muted text-center py-6">
              未配置 MCP 服务器。点击「合并 MCP (JSON)」粘贴配置，或在{' '}
              <code className="font-mono text-[11px]">~/.deepseek/mcp.json</code>{' '}
              中手动编辑后重启应用。
            </p>
          )}
        </div>
      )}

      {/* Tool list (when server selected) */}
      {selectedServer !== null && (
        <div className="overflow-y-auto px-3 py-2 space-y-1.5">
          {tools.map((t) => (
            <ToolRow key={`${t.server}/${t.name}`} tool={t} />
          ))}
          {tools.length === 0 && !loading && (
            <p className="text-xs text-t-text-muted text-center py-6">
              此服务器未公开任何工具。
            </p>
          )}
        </div>
      )}
      {/* Restart confirmation overlay */}
      {showRestartDialog && (
        <div className="absolute inset-0 bg-overlay flex items-center justify-center z-50">
          <div className="bg-card border border-card-border rounded-2xl p-6 mx-4 max-w-sm shadow-lg text-center">
            <p className="text-sm text-t-text mb-2 font-semibold">MCP 服务器配置已保存</p>
            <p className="text-xs text-t-text-secondary mb-5 leading-relaxed">
              新配置已写入 <code className="font-mono text-[11px]">~/.deepseek/mcp.json</code>，需要重启运行时以连接新服务器。是否立即重启？
            </p>
            <div className="flex justify-center gap-3">
              <button
                type="button"
                onClick={() => setShowRestartDialog(false)}
                className="px-4 py-2 rounded-lg text-xs text-t-text-muted hover:text-t-text hover:bg-hover"
              >
                稍后
              </button>
              <button
                type="button"
                onClick={handleRestartSidecar}
                disabled={restartPending}
                className="px-4 py-2 rounded-lg text-xs font-medium bg-accent text-accent-text hover:opacity-90 disabled:opacity-50"
              >
                {restartPending ? '重启中…' : '立即重启'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Merge MCP config JSON (same shapes as ~/.deepseek/mcp.json)        */
/* ------------------------------------------------------------------ */

const MCP_JSON_EXAMPLE = `{
  "my-filesystem": {
    "command": "npx",
    "args": ["-y", "@modelcontextprotocol/server-filesystem", "."]
  }
}`;

function AddMcpJsonForm({
  onSubmit,
  onCancel,
}: {
  onSubmit: (jsonText: string) => void | Promise<void>;
  onCancel: () => void;
}) {
  const [text, setText] = useState(MCP_JSON_EXAMPLE);
  const [busy, setBusy] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

  const handleSubmit = () => {
    setFormError(null);
    const trimmed = text.trim();
    if (!trimmed) {
      setFormError('请输入 JSON');
      return;
    }
    void (async () => {
      setBusy(true);
      try {
        await onSubmit(trimmed);
      } finally {
        setBusy(false);
      }
    })();
  };

  return (
    <div className="shrink-0 border-b border-divider px-3 py-3 space-y-2 bg-canvas-alt/50">
      <div className="text-[11px] font-semibold text-t-text-secondary">从 JSON 合并 MCP 配置</div>
      <p className="text-[10px] text-t-text-muted leading-relaxed">
        粘贴与 <code className="font-mono text-[10px]">~/.deepseek/mcp.json</code> 相同结构的片段：完整{' '}
        <code className="font-mono text-[10px]">mcpServers</code> /{' '}
        <code className="font-mono text-[10px]">servers</code>，或多个{' '}
        <code className="font-mono text-[10px]">&quot;名称&quot;: {'{ … }'}</code> 条目。同名服务器会被覆盖。
      </p>
      <textarea
        value={text}
        onChange={(e) => { setText(e.target.value); setFormError(null); }}
        spellCheck={false}
        rows={14}
        className="w-full px-2.5 py-2 text-[11px] font-mono leading-relaxed rounded-lg bg-input-bg border border-input-border text-t-text outline-none focus:border-accent resize-y min-h-[180px]"
        aria-label="MCP JSON"
      />
      {formError && <p className="text-[10px] text-t-error">{formError}</p>}
      <div className="flex items-center gap-2 pt-1">
        <button
          type="button"
          disabled={busy}
          onClick={handleSubmit}
          className="px-4 py-1.5 rounded text-xs font-medium bg-accent text-accent-text hover:opacity-90 disabled:opacity-50"
        >
          {busy ? '保存中…' : '合并并保存'}
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={onCancel}
          className="px-3 py-1.5 rounded text-xs text-t-text-muted hover:text-t-text hover:bg-hover"
        >
          取消
        </button>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Server card                                                        */
/* ------------------------------------------------------------------ */

function ServerCard({
  server,
  onSelect,
}: {
  server: McpServerEntry;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onSelect}
      className="w-full text-left p-3 rounded-lg border border-card-border bg-canvas-alt hover:bg-hover transition-colors"
    >
      <div className="flex items-center gap-2">
        <span className="text-sm font-semibold text-t-text">{server.name}</span>
        <span
          className={`ml-auto px-1.5 py-0.5 rounded text-[10px] font-medium ${
            server.connected
              ? 'bg-success-bg text-success'
              : 'bg-error-bg text-t-error'
          }`}
        >
          {server.connected ? '已连接' : '未连接'}
        </span>
      </div>
      <div className="mt-1 text-[11px] text-t-text-muted">
        {server.transport ?? (server.command ? 'stdio' : server.url ? 'sse' : '—')}
        {' · '}
        {server.tool_count} 个工具
        {server.required && (
          <span className="ml-2 text-[10px] text-amber-text">（必需）</span>
        )}
      </div>
    </button>
  );
}

/* ------------------------------------------------------------------ */
/*  Tool row                                                           */
/* ------------------------------------------------------------------ */

function ToolRow({ tool }: { tool: McpToolEntry }) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="rounded border border-divider bg-canvas-alt overflow-hidden">
      <button
        type="button"
        onClick={() => setExpanded(!expanded)}
        className="w-full text-left px-3 py-2 flex items-center gap-2 hover:bg-hover transition-colors"
      >
        <span className="font-mono text-xs text-accent">{tool.prefixed_name}</span>
        {tool.description && (
          <span className="text-[11px] text-t-text-muted truncate flex-1">
            — {tool.description}
          </span>
        )}
        <svg
          viewBox="0 0 24 24"
          className={`w-3.5 h-3.5 stroke-current text-t-text-muted transition-transform ${
            expanded ? 'rotate-90' : ''
          }`}
          style={{ fill: 'none', strokeWidth: 2 }}
        >
          <path d="M9 5l7 7-7 7" />
        </svg>
      </button>
      {expanded && (
        <div className="border-t border-divider px-3 py-2">
          <pre className="text-[10px] font-mono text-t-text-muted whitespace-pre-wrap max-h-32 overflow-auto leading-relaxed">
            {JSON.stringify(tool.input_schema, null, 2)}
          </pre>
        </div>
      )}
    </div>
  );
}
