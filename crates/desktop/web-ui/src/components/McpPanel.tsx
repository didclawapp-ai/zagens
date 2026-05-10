import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  fetchMcpServers,
  fetchMcpTools,
  mergeMcpConfigJson,
  addMcpServer,
  getMcpServer,
  putMcpServer,
  deleteMcpServer,
  invalidateRuntimeBootReadyCache,
  type RuntimeConnectionState,
} from '../api/client';
import type { McpServerEntry, McpToolEntry, McpServerConfigPayload } from '../types/mcp';

function emptyServerConfig(): McpServerConfigPayload {
  return {
    command: null,
    args: [],
    env: {},
    url: null,
    connect_timeout: null,
    execute_timeout: null,
    read_timeout: null,
    disabled: false,
    enabled: true,
    required: false,
    enabled_tools: [],
    disabled_tools: [],
  };
}

function normalizeServerConfig(raw: Partial<McpServerConfigPayload>): McpServerConfigPayload {
  const d = emptyServerConfig();
  return {
    ...d,
    ...raw,
    args: Array.isArray(raw.args) ? raw.args : d.args,
    env: raw.env && typeof raw.env === 'object' ? raw.env : d.env,
    enabled_tools: Array.isArray(raw.enabled_tools) ? raw.enabled_tools : d.enabled_tools,
    disabled_tools: Array.isArray(raw.disabled_tools) ? raw.disabled_tools : d.disabled_tools,
  };
}

export default function McpPanel({ runtimeConn }: { runtimeConn: RuntimeConnectionState }) {
  const [servers, setServers] = useState<McpServerEntry[]>([]);
  const [allTools, setAllTools] = useState<McpToolEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedServer, setSelectedServer] = useState<string | null>(null);
  const [showAddDialog, setShowAddDialog] = useState(false);
  const [showQuickAdd, setShowQuickAdd] = useState(false);
  const [showRestartDialog, setShowRestartDialog] = useState(false);
  const [restartPending, setRestartPending] = useState(false);
  const [editingServer, setEditingServer] = useState<string | null>(null);
  const [deletingServer, setDeletingServer] = useState<string | null>(null);

  const toolCountByServer = useMemo(() => {
    const m = new Map<string, number>();
    for (const t of allTools) {
      m.set(t.server, (m.get(t.server) ?? 0) + 1);
    }
    return m;
  }, [allTools]);

  const requestReloadRestart = useCallback(() => {
    setShowRestartDialog(true);
  }, []);

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [srv, tl] = await Promise.all([fetchMcpServers(), fetchMcpTools()]);
      setServers(srv.servers);
      setAllTools(tl.tools);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (runtimeConn === 'connected') {
      void reload();
    }
  }, [runtimeConn, reload]);

  const displayedTools =
    selectedServer === null ? [] : allTools.filter((t) => t.server === selectedServer);

  const handleMergeMcpJson = async (jsonText: string) => {
    setError(null);
    try {
      await mergeMcpConfigJson(jsonText);
      setShowAddDialog(false);
      requestReloadRestart();
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

  const openDeleteConfirm = (name: string) => {
    setDeletingServer(name);
    setError(null);
  };

  const confirmDelete = async () => {
    if (!deletingServer) return;
    setError(null);
    try {
      await deleteMcpServer(deletingServer);
      setDeletingServer(null);
      if (selectedServer === deletingServer) setSelectedServer(null);
      if (editingServer === deletingServer) setEditingServer(null);
      requestReloadRestart();
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setDeletingServer(null);
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
      <div className="flex items-center gap-2 px-3 py-2 border-b border-divider shrink-0 flex-wrap">
        <span className="text-[11px] text-t-text-muted">
          {servers.length} 个服务器 · {allTools.length} 个工具
        </span>
        <div className="flex-1" />
        <button
          type="button"
          onClick={() => {
            setShowQuickAdd((v) => !v);
            setShowAddDialog(false);
          }}
          className="px-2.5 py-1 rounded text-xs font-medium border border-card-border bg-canvas-alt hover:bg-hover text-t-text transition-colors"
        >
          {showQuickAdd ? '关闭添加' : '添加服务器'}
        </button>
        <button
          type="button"
          onClick={() => {
            setShowAddDialog(!showAddDialog);
            setShowQuickAdd(false);
          }}
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

      {showQuickAdd && (
        <QuickAddServerForm
          onSubmit={async (req) => {
            setError(null);
            try {
              await addMcpServer(req);
              setShowQuickAdd(false);
              requestReloadRestart();
              await reload();
            } catch (e) {
              setError(e instanceof Error ? e.message : String(e));
            }
          }}
          onCancel={() => setShowQuickAdd(false)}
        />
      )}

      {showAddDialog && (
        <AddMcpJsonForm
          onSubmit={handleMergeMcpJson}
          onCancel={() => setShowAddDialog(false)}
        />
      )}

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
          全部 ({allTools.length})
        </button>
        {servers.map((s) => {
          const c = toolCountByServer.get(s.name) ?? 0;
          return (
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
              <span className="ml-1 text-[10px] opacity-70">({c})</span>
            </button>
          );
        })}
      </div>

      {selectedServer === null && (
        <div className="overflow-y-auto px-3 py-2 space-y-2">
          {servers.map((s) => (
            <ServerCard
              key={s.name}
              server={s}
              toolCount={toolCountByServer.get(s.name) ?? 0}
              onSelectTools={() => setSelectedServer(s.name)}
              onEdit={() => {
                setEditingServer(s.name);
                setError(null);
              }}
              onDelete={() => openDeleteConfirm(s.name)}
            />
          ))}
          {servers.length === 0 && !loading && (
            <p className="text-xs text-t-text-muted text-center py-6">
              未配置 MCP 服务器。使用「添加服务器」或「合并 MCP (JSON)」，或在{' '}
              <code className="font-mono text-[11px]">~/.deepseek/mcp.json</code>{' '}
              中手动编辑后重启应用。
            </p>
          )}
        </div>
      )}

      {selectedServer !== null && (
        <div className="overflow-y-auto px-3 py-2 space-y-1.5">
          {displayedTools.map((t) => (
            <ToolRow key={`${t.server}/${t.name}`} tool={t} />
          ))}
          {displayedTools.length === 0 && !loading && (
            <p className="text-xs text-t-text-muted text-center py-6">
              此服务器未公开任何工具（或未连接）。
            </p>
          )}
        </div>
      )}

      {editingServer && (
        <EditMcpServerDialog
          serverName={editingServer}
          onClose={() => setEditingServer(null)}
          onSaved={async () => {
            setEditingServer(null);
            requestReloadRestart();
            await reload();
          }}
          onError={(msg) => setError(msg)}
        />
      )}

      {deletingServer && (
        <div className="absolute inset-0 bg-overlay flex items-center justify-center z-50">
          <div className="bg-card border border-card-border rounded-2xl p-6 mx-4 max-w-sm shadow-lg">
            <p className="text-sm text-t-text mb-2 font-semibold">删除 MCP 服务器</p>
            <p className="text-xs text-t-text-secondary mb-5 leading-relaxed">
              确定从配置中移除「{deletingServer}」？此操作会写入{' '}
              <code className="font-mono text-[11px]">mcp.json</code>，建议随后重启运行时。
            </p>
            <div className="flex justify-end gap-3">
              <button
                type="button"
                onClick={() => setDeletingServer(null)}
                className="px-4 py-2 rounded-lg text-xs text-t-text-muted hover:text-t-text hover:bg-hover"
              >
                取消
              </button>
              <button
                type="button"
                onClick={() => void confirmDelete()}
                className="px-4 py-2 rounded-lg text-xs font-medium bg-t-error text-white hover:opacity-90"
              >
                删除
              </button>
            </div>
          </div>
        </div>
      )}

      {showRestartDialog && (
        <div className="absolute inset-0 bg-overlay flex items-center justify-center z-50">
          <div className="bg-card border border-card-border rounded-2xl p-6 mx-4 max-w-sm shadow-lg text-center">
            <p className="text-sm text-t-text mb-2 font-semibold">MCP 服务器配置已保存</p>
            <p className="text-xs text-t-text-secondary mb-5 leading-relaxed">
              新配置已写入 <code className="font-mono text-[11px]">~/.deepseek/mcp.json</code>
              ，需要重启运行时以重新连接服务器。是否立即重启？
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
/*  Quick add (POST /v1/apps/mcp/servers)                              */
/* ------------------------------------------------------------------ */

function QuickAddServerForm({
  onSubmit,
  onCancel,
}: {
  onSubmit: (req: { name: string; command?: string; url?: string; args: string[] }) => void | Promise<void>;
  onCancel: () => void;
}) {
  const [name, setName] = useState('');
  const [command, setCommand] = useState('');
  const [url, setUrl] = useState('');
  const [argsText, setArgsText] = useState('');
  const [busy, setBusy] = useState(false);
  const [localErr, setLocalErr] = useState<string | null>(null);

  const handleSubmit = () => {
    setLocalErr(null);
    const n = name.trim();
    if (!n) {
      setLocalErr('请填写服务器名称');
      return;
    }
    const cmd = command.trim();
    const u = url.trim();
    if (!cmd && !u) {
      setLocalErr('请填写 command（stdio）或 URL（远程）之一');
      return;
    }
    const args = argsText
      .split('\n')
      .map((l) => l.trimEnd())
      .filter((l) => l.length > 0);
    void (async () => {
      setBusy(true);
      try {
        await onSubmit({
          name: n,
          ...(cmd ? { command: cmd } : {}),
          ...(u ? { url: u } : {}),
          args,
        });
      } finally {
        setBusy(false);
      }
    })();
  };

  return (
    <div className="shrink-0 border-b border-divider px-3 py-3 space-y-2 bg-canvas-alt/50">
      <div className="text-[11px] font-semibold text-t-text-secondary">快速添加 MCP 服务器</div>
      <p className="text-[10px] text-t-text-muted leading-relaxed">
        与终端配置相同：stdio 填写可执行命令，远程填写 <span className="font-mono">url</span>。参数每行一项。
      </p>
      <input
        type="text"
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder="服务器名称（唯一）"
        className="w-full px-2.5 py-1.5 text-xs rounded-lg bg-input-bg border border-input-border text-t-text"
      />
      <input
        type="text"
        value={command}
        onChange={(e) => setCommand(e.target.value)}
        placeholder="command（例如 npx）"
        className="w-full px-2.5 py-1.5 text-xs rounded-lg bg-input-bg border border-input-border text-t-text font-mono"
      />
      <input
        type="text"
        value={url}
        onChange={(e) => setUrl(e.target.value)}
        placeholder="url（与 command 二选一）"
        className="w-full px-2.5 py-1.5 text-xs rounded-lg bg-input-bg border border-input-border text-t-text font-mono"
      />
      <textarea
        value={argsText}
        onChange={(e) => setArgsText(e.target.value)}
        placeholder={'参数，每行一项（可选）\n例如 -y\n@modelcontextprotocol/server-filesystem'}
        rows={4}
        className="w-full px-2.5 py-2 text-[11px] font-mono rounded-lg bg-input-bg border border-input-border text-t-text resize-y min-h-[80px]"
      />
      {localErr && <p className="text-[10px] text-t-error">{localErr}</p>}
      <div className="flex items-center gap-2 pt-1">
        <button
          type="button"
          disabled={busy}
          onClick={handleSubmit}
          className="px-4 py-1.5 rounded text-xs font-medium bg-accent text-accent-text hover:opacity-90 disabled:opacity-50"
        >
          {busy ? '保存中…' : '保存'}
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
/*  Merge MCP config JSON                                              */
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
        onChange={(e) => {
          setText(e.target.value);
          setFormError(null);
        }}
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
  toolCount,
  onSelectTools,
  onEdit,
  onDelete,
}: {
  server: McpServerEntry;
  toolCount: number;
  onSelectTools: () => void;
  onEdit: () => void;
  onDelete: () => void;
}) {
  const args = server.args ?? [];
  const transport = server.command ? 'stdio' : server.url ? 'remote' : '—';

  return (
    <div className="rounded-lg border border-card-border bg-canvas-alt p-3 space-y-2">
      <div className="flex items-start gap-2">
        <div className="flex-1 min-w-0">
          <button
            type="button"
            onClick={onSelectTools}
            className="text-sm font-semibold text-t-text hover:text-accent text-left truncate max-w-full"
          >
            {server.name}
          </button>
          <div className="mt-1 text-[11px] text-t-text-muted">
            {transport}
            {' · '}
            {toolCount} 个工具
            {args.length > 0 && (
              <span className="block mt-0.5 font-mono text-[10px] opacity-80 truncate" title={args.join(' ')}>
                {server.command ? `${server.command} ` : ''}
                {args.join(' ')}
              </span>
            )}
            {server.required && <span className="ml-2 text-[10px] text-amber-text">（必需）</span>}
          </div>
        </div>
        <span
          className={`shrink-0 px-1.5 py-0.5 rounded text-[10px] font-medium ${
            server.connected
              ? 'bg-success-bg text-success'
              : 'bg-error-bg text-t-error'
          }`}
        >
          {server.connected ? '已连接' : '未连接'}
        </span>
      </div>
      <div className="flex flex-wrap gap-2">
        <button
          type="button"
          onClick={onSelectTools}
          className="px-2 py-1 rounded text-[10px] font-medium border border-divider hover:bg-hover text-t-text"
        >
          查看工具
        </button>
        <button
          type="button"
          onClick={onEdit}
          className="px-2 py-1 rounded text-[10px] font-medium border border-divider hover:bg-hover text-accent"
        >
          编辑
        </button>
        <button
          type="button"
          onClick={onDelete}
          className="px-2 py-1 rounded text-[10px] font-medium border border-divider hover:bg-hover text-t-error"
        >
          删除
        </button>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Edit server (GET + PUT)                                            */
/* ------------------------------------------------------------------ */

function EditMcpServerDialog({
  serverName,
  onClose,
  onSaved,
  onError,
}: {
  serverName: string;
  onClose: () => void;
  onSaved: () => void | Promise<void>;
  onError: (msg: string) => void;
}) {
  const [cfg, setCfg] = useState<McpServerConfigPayload | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [argsText, setArgsText] = useState('');
  const [envText, setEnvText] = useState('{}');
  const [enabledToolsText, setEnabledToolsText] = useState('');
  const [disabledToolsText, setDisabledToolsText] = useState('');

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const raw = await getMcpServer(serverName);
        if (cancelled) return;
        const n = normalizeServerConfig(raw);
        setCfg(n);
        setArgsText(n.args.join('\n'));
        setEnvText(JSON.stringify(n.env ?? {}, null, 2));
        setEnabledToolsText(n.enabled_tools.join(', '));
        setDisabledToolsText(n.disabled_tools.join(', '));
        setLoadError(null);
      } catch (e) {
        if (!cancelled) {
          setLoadError(e instanceof Error ? e.message : String(e));
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [serverName]);

  const save = () => {
    if (!cfg) return;
    const cmd = (cfg.command ?? '').trim();
    const u = (cfg.url ?? '').trim();
    if (!cmd && !u) {
      onError('请填写 command 或 url');
      return;
    }
    let env: Record<string, string>;
    try {
      const parsed = JSON.parse(envText.trim() || '{}') as unknown;
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
        onError('环境变量须为 JSON 对象');
        return;
      }
      env = parsed as Record<string, string>;
    } catch {
      onError('环境变量 JSON 无效');
      return;
    }
    const args = argsText
      .split('\n')
      .map((l) => l.trimEnd())
      .filter((l) => l.length > 0);
    const enabled_tools = enabledToolsText
      .split(/[,，]/g)
      .map((s) => s.trim())
      .filter(Boolean);
    const disabled_tools = disabledToolsText
      .split(/[,，]/g)
      .map((s) => s.trim())
      .filter(Boolean);

    const payload: McpServerConfigPayload = {
      ...cfg,
      command: cmd || null,
      url: u || null,
      args,
      env,
      enabled_tools,
      disabled_tools,
      connect_timeout: cfg.connect_timeout ?? null,
      execute_timeout: cfg.execute_timeout ?? null,
      read_timeout: cfg.read_timeout ?? null,
    };

    void (async () => {
      setBusy(true);
      try {
        await putMcpServer(serverName, payload);
        await onSaved();
      } catch (e) {
        onError(e instanceof Error ? e.message : String(e));
      } finally {
        setBusy(false);
      }
    })();
  };

  return (
    <div className="absolute inset-0 bg-overlay flex items-center justify-center z-50 p-3">
      <div className="bg-card border border-card-border rounded-2xl max-w-lg w-full max-h-[90vh] overflow-hidden flex flex-col shadow-lg">
        <div className="px-4 py-3 border-b border-divider shrink-0 flex items-center gap-2">
          <h3 className="text-sm font-semibold text-t-text flex-1 truncate">编辑 MCP · {serverName}</h3>
          <button
            type="button"
            onClick={onClose}
            className="text-[11px] text-t-text-muted hover:text-t-text px-2 py-1 rounded hover:bg-hover"
          >
            关闭
          </button>
        </div>
        <div className="overflow-y-auto px-4 py-3 space-y-3 text-xs">
          {loadError && <p className="text-[11px] text-t-error">{loadError}</p>}
          {!cfg && !loadError && <p className="text-[11px] text-t-text-muted">加载中…</p>}
          {cfg && (
            <>
              <label className="flex flex-col gap-1 text-[10px] text-t-text-muted">
                command（stdio）
                <input
                  type="text"
                  value={cfg.command ?? ''}
                  onChange={(e) => setCfg({ ...cfg, command: e.target.value || null })}
                  className="px-2 py-1.5 rounded-lg bg-input-bg border border-input-border text-t-text font-mono text-[11px]"
                />
              </label>
              <label className="flex flex-col gap-1 text-[10px] text-t-text-muted">
                url（远程，与 command 二选一）
                <input
                  type="text"
                  value={cfg.url ?? ''}
                  onChange={(e) => setCfg({ ...cfg, url: e.target.value || null })}
                  className="px-2 py-1.5 rounded-lg bg-input-bg border border-input-border text-t-text font-mono text-[11px]"
                />
              </label>
              <label className="flex flex-col gap-1 text-[10px] text-t-text-muted">
                args（每行一项）
                <textarea
                  value={argsText}
                  onChange={(e) => setArgsText(e.target.value)}
                  rows={5}
                  className="px-2 py-1.5 rounded-lg bg-input-bg border border-input-border text-t-text font-mono text-[11px] resize-y min-h-[72px]"
                />
              </label>
              <label className="flex flex-col gap-1 text-[10px] text-t-text-muted">
                env（JSON 对象）
                <textarea
                  value={envText}
                  onChange={(e) => setEnvText(e.target.value)}
                  rows={4}
                  spellCheck={false}
                  className="px-2 py-1.5 rounded-lg bg-input-bg border border-input-border text-t-text font-mono text-[11px] resize-y min-h-[64px]"
                />
              </label>
              <div className="grid grid-cols-3 gap-2">
                <label className="inline-flex items-center gap-1.5 text-[10px] text-t-text cursor-pointer">
                  <input
                    type="checkbox"
                    checked={cfg.enabled}
                    onChange={(e) => setCfg({ ...cfg, enabled: e.target.checked })}
                  />
                  enabled
                </label>
                <label className="inline-flex items-center gap-1.5 text-[10px] text-t-text cursor-pointer">
                  <input
                    type="checkbox"
                    checked={cfg.disabled}
                    onChange={(e) => setCfg({ ...cfg, disabled: e.target.checked })}
                  />
                  disabled
                </label>
                <label className="inline-flex items-center gap-1.5 text-[10px] text-t-text cursor-pointer">
                  <input
                    type="checkbox"
                    checked={cfg.required}
                    onChange={(e) => setCfg({ ...cfg, required: e.target.checked })}
                  />
                  required
                </label>
              </div>
              <label className="flex flex-col gap-1 text-[10px] text-t-text-muted">
                enabled_tools（逗号分隔，留空表示全部允许）
                <input
                  type="text"
                  value={enabledToolsText}
                  onChange={(e) => setEnabledToolsText(e.target.value)}
                  className="px-2 py-1.5 rounded-lg bg-input-bg border border-input-border text-t-text text-[11px]"
                />
              </label>
              <label className="flex flex-col gap-1 text-[10px] text-t-text-muted">
                disabled_tools（逗号分隔）
                <input
                  type="text"
                  value={disabledToolsText}
                  onChange={(e) => setDisabledToolsText(e.target.value)}
                  className="px-2 py-1.5 rounded-lg bg-input-bg border border-input-border text-t-text text-[11px]"
                />
              </label>
            </>
          )}
        </div>
        <div className="px-4 py-3 border-t border-divider shrink-0 flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            disabled={busy}
            className="px-3 py-1.5 rounded-lg text-[11px] text-t-text-muted hover:bg-hover"
          >
            取消
          </button>
          <button
            type="button"
            disabled={busy || !cfg}
            onClick={save}
            className="px-4 py-1.5 rounded-lg text-[11px] font-medium bg-accent text-accent-text hover:opacity-90 disabled:opacity-40"
          >
            {busy ? '保存中…' : '保存'}
          </button>
        </div>
      </div>
    </div>
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
          <span className="text-[11px] text-t-text-muted truncate flex-1">— {tool.description}</span>
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
