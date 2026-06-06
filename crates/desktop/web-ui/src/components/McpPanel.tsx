import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  fetchMcpServers,
  fetchMcpTools,
  fetchMcpDiscover,
  mergeMcpConfigJson,
  reloadMcpConfig,
  getMcpServer,
  putMcpServer,
  deleteMcpServer,
  type RuntimeConnectionState,
} from '../api/client';
import { useT } from '../i18n';
import { isRuntimeApiAvailable } from '../lib/runtimeReachable';
import type {
  McpCallRecord,
  McpServerDiscoverEntry,
  McpServerEntry,
  McpToolEntry,
  McpServerConfigPayload,
} from '../types/mcp';
import McpServerDetail from './McpServerDetail';

function emptyServerConfig(): McpServerConfigPayload {
  return {
    command: null,
    args: [],
    env: {},
    url: null,
    transport: null,
    headers: {},
    auth: null,
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
    headers: raw.headers && typeof raw.headers === 'object' ? raw.headers : d.headers,
    enabled_tools: Array.isArray(raw.enabled_tools) ? raw.enabled_tools : d.enabled_tools,
    disabled_tools: Array.isArray(raw.disabled_tools) ? raw.disabled_tools : d.disabled_tools,
  };
}

function formatHeadersText(headers: Record<string, string> | undefined): string {
  if (!headers) return '';
  return Object.entries(headers)
    .map(([k, v]) => `${k}: ${v}`)
    .join('\n');
}

function parseHeadersText(text: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const line of text.split('\n')) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    const idx = trimmed.indexOf(':');
    if (idx <= 0) continue;
    const name = trimmed.slice(0, idx).trim();
    const value = trimmed.slice(idx + 1).trim();
    if (name) out[name] = value;
  }
  return out;
}

export default function McpPanel({
  runtimeConn,
  streaming = false,
  runtimeSessionEstablished = false,
}: {
  runtimeConn: RuntimeConnectionState;
  streaming?: boolean;
  runtimeSessionEstablished?: boolean;
}) {
  const { t } = useT();
  const runtimeReady = isRuntimeApiAvailable(runtimeConn, {
    streaming,
    sessionEstablished: runtimeSessionEstablished,
  });
  const [servers, setServers] = useState<McpServerEntry[]>([]);
  const [allTools, setAllTools] = useState<McpToolEntry[]>([]);
  const [discoverByServer, setDiscoverByServer] = useState<Map<string, McpServerDiscoverEntry>>(
    new Map(),
  );
  const [recentCalls, setRecentCalls] = useState<McpCallRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [discovering, setDiscovering] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedServer, setSelectedServer] = useState<string | null>(null);
  const [togglingTool, setTogglingTool] = useState<string | null>(null);
  const [showAddJson, setShowAddJson] = useState(false);
  const [editingServer, setEditingServer] = useState<string | null>(null);
  const [deletingServer, setDeletingServer] = useState<string | null>(null);

  const toolCountByServer = useMemo(() => {
    const m = new Map<string, number>();
    for (const t of allTools) {
      m.set(t.server, (m.get(t.server) ?? 0) + 1);
    }
    return m;
  }, [allTools]);

  const applyDiscover = useCallback((snapshot: { servers: McpServerDiscoverEntry[] }) => {
    const m = new Map<string, McpServerDiscoverEntry>();
    for (const s of snapshot.servers) m.set(s.name, s);
    setDiscoverByServer(m);
  }, []);

  const reload = useCallback(async (opts?: { discover?: boolean }) => {
    const withDiscover = opts?.discover !== false;
    setLoading(true);
    setError(null);
    if (withDiscover) setDiscovering(true);
    try {
      const [srv, tl, disc] = await Promise.all([
        fetchMcpServers(),
        fetchMcpTools(),
        withDiscover ? fetchMcpDiscover() : Promise.resolve(null),
      ]);
      if (disc) {
        applyDiscover(disc.snapshot);
        setRecentCalls(disc.recent_calls);
        const connectedByName = new Map(
          disc.snapshot.servers.map((s) => [s.name, s.connected] as const),
        );
        setServers(
          srv.servers.map((s) => ({
            ...s,
            connected: connectedByName.get(s.name) ?? s.connected,
          })),
        );
      } else {
        setServers(srv.servers);
      }
      setAllTools(tl.tools);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
      setDiscovering(false);
    }
  }, [applyDiscover]);

  const pollServers = useCallback(async () => {
    try {
      const srv = await fetchMcpServers();
      setServers(srv.servers);
    } catch {
      /* ignore background poll errors */
    }
  }, []);

  const applyMcpHotReload = useCallback(async () => {
    setError(null);
    try {
      await reloadMcpConfig();
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [reload]);

  useEffect(() => {
    if (runtimeReady) {
      void reload();
    }
  }, [runtimeReady, reload]);

  useEffect(() => {
    if (runtimeReady && !loading && servers.length === 0) {
      setShowAddJson(true);
    }
  }, [runtimeReady, loading, servers.length]);

  useEffect(() => {
    if (!runtimeReady) return;
    const id = window.setInterval(() => void pollServers(), 20_000);
    return () => window.clearInterval(id);
  }, [runtimeReady, pollServers]);

  const selectedServerEntry = useMemo(
    () => (selectedServer ? servers.find((s) => s.name === selectedServer) : undefined),
    [servers, selectedServer],
  );

  const selectedDiscover = selectedServer ? discoverByServer.get(selectedServer) : undefined;

  const handleToggleTool = useCallback(
    async (toolName: string, enable: boolean) => {
      if (!selectedServer) return;
      setTogglingTool(toolName);
      setError(null);
      try {
        const cfg = await getMcpServer(selectedServer);
        let enabledTools = [...cfg.enabled_tools];
        let disabledTools = [...cfg.disabled_tools];
        if (enable) {
          disabledTools = disabledTools.filter((t) => t !== toolName);
          if (enabledTools.length > 0 && !enabledTools.includes(toolName)) {
            enabledTools.push(toolName);
          }
        } else {
          if (!disabledTools.includes(toolName)) disabledTools.push(toolName);
          enabledTools = enabledTools.filter((t) => t !== toolName);
        }
        await putMcpServer(selectedServer, {
          ...cfg,
          enabled_tools: enabledTools,
          disabled_tools: disabledTools,
        });
        await reloadMcpConfig();
        await reload();
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setTogglingTool(null);
      }
    },
    [selectedServer, reload],
  );

  const handleMergeMcpJson = async (jsonText: string) => {
    setError(null);
    try {
      await mergeMcpConfigJson(jsonText);
      setShowAddJson(false);
      await applyMcpHotReload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
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
      await applyMcpHotReload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setDeletingServer(null);
    }
  };

  if (!runtimeReady) {
    return (
      <div className="p-4 text-xs text-t-text-muted text-center space-y-2">
        <p>{t('mcp.waitingRuntime')}</p>
        <p className="text-[10px]">{t('mcp.waitingDetail')}</p>
      </div>
    );
  }

  if (loading && servers.length === 0 && !error) {
    return (
      <div className="p-4 text-xs text-t-text-muted text-center">
        <p>{t('mcp.loading')}</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full overflow-hidden relative">
      <div className="flex items-center gap-2 px-3 py-2 border-b border-divider shrink-0 flex-wrap">
        <span className="text-[11px] text-t-text-muted">
          {t('mcp.serversAndTools', {
            servers: String(servers.length),
            tools: String(allTools.length),
          })}
        </span>
        <div className="flex-1" />
        <button
          type="button"
          onClick={() => void applyMcpHotReload()}
          className="px-2.5 py-1 rounded text-xs font-medium border border-card-border bg-canvas-alt hover:bg-hover text-t-text transition-colors"
          title={t('mcp.applyConfigHint')}
        >
          {t('mcp.applyConfig')}
        </button>
        <button
          type="button"
          onClick={() => setShowAddJson((v) => !v)}
          className="px-2.5 py-1 rounded text-xs font-medium bg-accent text-accent-text hover:opacity-90 transition-opacity"
        >
          {showAddJson ? t('mcp.closeAdd') : t('mcp.addServer')}
        </button>
      </div>

      {error && (
        <p className="shrink-0 px-3 py-1.5 text-[11px] text-t-error bg-error-bg/30 border-b border-divider">
          {error}
        </p>
      )}

      {showAddJson && (
        <AddMcpJsonForm
          onSubmit={handleMergeMcpJson}
          onCancel={() => setShowAddJson(false)}
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
          {t('mcp.allTools', { count: String(allTools.length) })}
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
              {t('mcp.noServers')}
            </p>
          )}
        </div>
      )}

      {selectedServer !== null && selectedServerEntry && (
        <McpServerDetail
          server={selectedServerEntry}
          discover={selectedDiscover}
          tools={allTools}
          recentCalls={recentCalls}
          togglingTool={togglingTool}
          onToggleTool={handleToggleTool}
        />
      )}

      {discovering && selectedServer !== null && (
        <p className="text-[10px] text-t-text-muted text-center py-1 shrink-0">
          {t('mcp.discovering')}
        </p>
      )}

      {editingServer && (
        <EditMcpServerDialog
          serverName={editingServer}
          onClose={() => setEditingServer(null)}
          onSaved={async () => {
            setEditingServer(null);
            await applyMcpHotReload();
          }}
          onError={(msg) => setError(msg)}
        />
      )}

      {deletingServer && (
        <div className="absolute inset-0 bg-overlay flex items-center justify-center z-50">
          <div className="bg-card border border-card-border rounded-2xl p-6 mx-4 max-w-sm shadow-lg">
            <p className="text-sm text-t-text mb-2 font-semibold">{t('mcp.deleteTitle')}</p>
            <p className="text-xs text-t-text-secondary mb-5 leading-relaxed">
              {t('mcp.deleteConfirm', { name: deletingServer })}
            </p>
            <div className="flex justify-end gap-3">
              <button
                type="button"
                onClick={() => setDeletingServer(null)}
                className="px-4 py-2 rounded-lg text-xs text-t-text-muted hover:text-t-text hover:bg-hover"
              >
                {t('mcp.cancel')}
              </button>
              <button
                type="button"
                onClick={() => void confirmDelete()}
                className="px-4 py-2 rounded-lg text-xs font-medium bg-t-error text-white hover:opacity-90"
              >
                {t('mcp.delete')}
              </button>
            </div>
          </div>
        </div>
      )}

    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Add MCP servers via JSON (same shape as ~/.zagens/mcp.json)        */
/* ------------------------------------------------------------------ */

const MCP_JSON_EXAMPLE = `{
  "everything": {
    "command": "npx",
    "args": ["-y", "@modelcontextprotocol/server-everything"]
  }
}`;

function AddMcpJsonForm({
  onSubmit,
  onCancel,
}: {
  onSubmit: (jsonText: string) => void | Promise<void>;
  onCancel: () => void;
}) {
  const { t } = useT();
  const [text, setText] = useState(MCP_JSON_EXAMPLE);
  const [busy, setBusy] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

  const handleSubmit = () => {
    setFormError(null);
    const trimmed = text.trim();
    if (!trimmed) {
      setFormError(t('mcp.jsonRequired'));
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
      <p className="text-[10px] text-t-text-muted leading-relaxed">{t('mcp.addJsonHint')}</p>
      <textarea
        value={text}
        onChange={(e) => {
          setText(e.target.value);
          setFormError(null);
        }}
        spellCheck={false}
        rows={10}
        className="w-full px-2.5 py-2 text-[11px] font-mono leading-relaxed rounded-lg bg-input-bg border border-input-border text-t-text outline-none focus:border-accent resize-y min-h-[140px]"
        aria-label={t('mcp.jsonAriaLabel')}
      />
      {formError && <p className="text-[10px] text-t-error">{formError}</p>}
      <div className="flex items-center gap-2 pt-1">
        <button
          type="button"
          disabled={busy}
          onClick={handleSubmit}
          className="px-4 py-1.5 rounded text-xs font-medium bg-accent text-accent-text hover:opacity-90 disabled:opacity-50"
        >
          {busy ? t('common.saving') : t('mcp.addAndApply')}
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={onCancel}
          className="px-3 py-1.5 rounded text-xs text-t-text-muted hover:text-t-text hover:bg-hover"
        >
          {t('common.cancel')}
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
  const { t } = useT();
  const args = server.args ?? [];
  const transport =
    server.transport ?? (server.command ? 'stdio' : server.url ? 'remote' : '—');

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
            {t('mcp.toolCount', { count: String(toolCount) })}
            {args.length > 0 && (
              <span className="block mt-0.5 font-mono text-[10px] opacity-80 truncate" title={args.join(' ')}>
                {server.command ? `${server.command} ` : ''}
                {args.join(' ')}
              </span>
            )}
            {server.required && <span className="ml-2 text-[10px] text-amber-text">{t('mcp.requiredBadge')}</span>}
          </div>
        </div>
        <span
          className={`shrink-0 px-1.5 py-0.5 rounded text-[10px] font-medium ${
            server.connected
              ? 'bg-success-bg text-success'
              : 'bg-error-bg text-t-error'
          }`}
        >
          {server.connected ? t('mcp.connected') : t('mcp.disconnected')}
        </span>
      </div>
      <div className="flex flex-wrap gap-2">
        <button
          type="button"
          onClick={onSelectTools}
          className="px-2 py-1 rounded text-[10px] font-medium border border-divider hover:bg-hover text-t-text"
        >
          {t('mcp.viewTools')}
        </button>
        <button
          type="button"
          onClick={onEdit}
          className="px-2 py-1 rounded text-[10px] font-medium border border-divider hover:bg-hover text-accent"
        >
          {t('mcp.edit')}
        </button>
        <button
          type="button"
          onClick={onDelete}
          className="px-2 py-1 rounded text-[10px] font-medium border border-divider hover:bg-hover text-t-error"
        >
          {t('mcp.delete')}
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
  const { t } = useT();
  const [cfg, setCfg] = useState<McpServerConfigPayload | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [argsText, setArgsText] = useState('');
  const [envText, setEnvText] = useState('{}');
  const [enabledToolsText, setEnabledToolsText] = useState('');
  const [disabledToolsText, setDisabledToolsText] = useState('');
  const [headersText, setHeadersText] = useState('');
  const [authType, setAuthType] = useState('');
  const [authToken, setAuthToken] = useState('');

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
        setHeadersText(formatHeadersText(n.headers));
        setAuthType(n.auth?.type ?? '');
        setAuthToken(n.auth?.token ?? n.auth?.apiKey ?? '');
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
      onError(t('mcp.commandOrUrlRequired'));
      return;
    }
    let env: Record<string, string>;
    try {
      const parsed = JSON.parse(envText.trim() || '{}') as unknown;
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
        onError(t('mcp.envMustBeObject'));
        return;
      }
      env = parsed as Record<string, string>;
    } catch {
      onError(t('mcp.envJsonInvalid'));
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

    const headers = parseHeadersText(headersText);
    const authTypeTrim = authType.trim().toLowerCase();
    const auth =
      authTypeTrim.length > 0
        ? {
            type: authTypeTrim,
            token: authToken.trim() || null,
            header: cfg.auth?.header ?? null,
            apiKey:
              authTypeTrim === 'apikey' || authTypeTrim === 'api_key'
                ? authToken.trim() || null
                : cfg.auth?.apiKey ?? null,
          }
        : null;

    const payload: McpServerConfigPayload = {
      ...cfg,
      command: cmd || null,
      url: u || null,
      args,
      env,
      headers,
      auth,
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
          <h3 className="text-sm font-semibold text-t-text flex-1 truncate">
            {t('mcp.editTitle', { name: serverName })}
          </h3>
          <button
            type="button"
            onClick={onClose}
            className="text-[11px] text-t-text-muted hover:text-t-text px-2 py-1 rounded hover:bg-hover"
          >
            {t('common.close')}
          </button>
        </div>
        <div className="overflow-y-auto px-4 py-3 space-y-3 text-xs">
          {loadError && <p className="text-[11px] text-t-error">{loadError}</p>}
          {!cfg && !loadError && <p className="text-[11px] text-t-text-muted">{t('common.loading')}</p>}
          {cfg && (
            <>
              <label className="flex flex-col gap-1 text-[10px] text-t-text-muted">
                {t('mcp.editCommandLabel')}
                <input
                  type="text"
                  value={cfg.command ?? ''}
                  onChange={(e) => setCfg({ ...cfg, command: e.target.value || null })}
                  className="px-2 py-1.5 rounded-lg bg-input-bg border border-input-border text-t-text font-mono text-[11px]"
                />
              </label>
              <label className="flex flex-col gap-1 text-[10px] text-t-text-muted">
                {t('mcp.editUrlLabel')}
                <input
                  type="text"
                  value={cfg.url ?? ''}
                  onChange={(e) => setCfg({ ...cfg, url: e.target.value || null })}
                  className="px-2 py-1.5 rounded-lg bg-input-bg border border-input-border text-t-text font-mono text-[11px]"
                />
              </label>
              <label className="flex flex-col gap-1 text-[10px] text-t-text-muted">
                {t('mcp.editArgsLabel')}
                <textarea
                  value={argsText}
                  onChange={(e) => setArgsText(e.target.value)}
                  rows={5}
                  className="px-2 py-1.5 rounded-lg bg-input-bg border border-input-border text-t-text font-mono text-[11px] resize-y min-h-[72px]"
                />
              </label>
              <label className="flex flex-col gap-1 text-[10px] text-t-text-muted">
                {t('mcp.editEnvLabel')}
                <textarea
                  value={envText}
                  onChange={(e) => setEnvText(e.target.value)}
                  rows={4}
                  spellCheck={false}
                  className="px-2 py-1.5 rounded-lg bg-input-bg border border-input-border text-t-text font-mono text-[11px] resize-y min-h-[64px]"
                />
              </label>
              <label className="flex flex-col gap-1 text-[10px] text-t-text-muted">
                {t('mcp.editHeadersLabel')}
                <textarea
                  value={headersText}
                  onChange={(e) => setHeadersText(e.target.value)}
                  rows={3}
                  spellCheck={false}
                  placeholder={t('mcp.headersPlaceholder')}
                  className="px-2 py-1.5 rounded-lg bg-input-bg border border-input-border text-t-text font-mono text-[11px] resize-y min-h-[56px]"
                />
              </label>
              <div className="grid grid-cols-2 gap-2">
                <label className="flex flex-col gap-1 text-[10px] text-t-text-muted">
                  {t('mcp.editAuthTypeLabel')}
                  <select
                    value={authType}
                    onChange={(e) => setAuthType(e.target.value)}
                    className="px-2 py-1.5 rounded-lg bg-input-bg border border-input-border text-t-text text-[11px]"
                  >
                    <option value="">—</option>
                    <option value="bearer">bearer</option>
                    <option value="apiKey">apiKey</option>
                  </select>
                </label>
                <label className="flex flex-col gap-1 text-[10px] text-t-text-muted">
                  {t('mcp.editAuthTokenLabel')}
                  <input
                    type="password"
                    value={authToken}
                    onChange={(e) => setAuthToken(e.target.value)}
                    placeholder={t('mcp.authTokenPlaceholder')}
                    autoComplete="off"
                    className="px-2 py-1.5 rounded-lg bg-input-bg border border-input-border text-t-text font-mono text-[11px]"
                  />
                </label>
              </div>
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
              <div className="grid grid-cols-3 gap-2">
                <label className="flex flex-col gap-1 text-[10px] text-t-text-muted">
                  {t('mcp.editConnectTimeoutLabel')}
                  <input
                    type="number"
                    min={1}
                    max={300}
                    placeholder="30"
                    value={cfg.connect_timeout ?? ''}
                    onChange={(e) =>
                      setCfg({
                        ...cfg,
                        connect_timeout: e.target.value ? Number(e.target.value) : null,
                      })
                    }
                    className="px-2 py-1.5 rounded-lg bg-input-bg border border-input-border text-t-text text-[11px] w-full"
                  />
                </label>
                <label className="flex flex-col gap-1 text-[10px] text-t-text-muted">
                  {t('mcp.editExecuteTimeoutLabel')}
                  <input
                    type="number"
                    min={1}
                    max={3600}
                    placeholder="60"
                    value={cfg.execute_timeout ?? ''}
                    onChange={(e) =>
                      setCfg({
                        ...cfg,
                        execute_timeout: e.target.value ? Number(e.target.value) : null,
                      })
                    }
                    className="px-2 py-1.5 rounded-lg bg-input-bg border border-input-border text-t-text text-[11px] w-full"
                  />
                </label>
                <label className="flex flex-col gap-1 text-[10px] text-t-text-muted">
                  {t('mcp.editReadTimeoutLabel')}
                  <input
                    type="number"
                    min={1}
                    max={3600}
                    placeholder="120"
                    value={cfg.read_timeout ?? ''}
                    onChange={(e) =>
                      setCfg({
                        ...cfg,
                        read_timeout: e.target.value ? Number(e.target.value) : null,
                      })
                    }
                    className="px-2 py-1.5 rounded-lg bg-input-bg border border-input-border text-t-text text-[11px] w-full"
                  />
                </label>
              </div>
              <label className="flex flex-col gap-1 text-[10px] text-t-text-muted">
                {t('mcp.editEnabledToolsLabel')}
                <input
                  type="text"
                  value={enabledToolsText}
                  onChange={(e) => setEnabledToolsText(e.target.value)}
                  className="px-2 py-1.5 rounded-lg bg-input-bg border border-input-border text-t-text text-[11px]"
                />
              </label>
              <label className="flex flex-col gap-1 text-[10px] text-t-text-muted">
                {t('mcp.editDisabledToolsLabel')}
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
            {t('common.cancel')}
          </button>
          <button
            type="button"
            disabled={busy || !cfg}
            onClick={save}
            className="px-4 py-1.5 rounded-lg text-[11px] font-medium bg-accent text-accent-text hover:opacity-90 disabled:opacity-40"
          >
            {busy ? t('common.saving') : t('common.save')}
          </button>
        </div>
      </div>
    </div>
  );
}

