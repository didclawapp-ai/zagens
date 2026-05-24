import { useCallback, useEffect, useMemo, useState } from 'react';
import type { RuntimeConnectionState } from '../api/client';
import { fetchTopicMemory, type TopicMemorySnapshot } from '../api/client';
import { useT } from '../i18n';
import { isRuntimeApiAvailable } from '../lib/runtimeReachable';

interface Props {
  runtimeConn: RuntimeConnectionState;
  streaming?: boolean;
  runtimeSessionEstablished?: boolean;
}

function strengthPct(strength: number): number {
  return Math.min(100, Math.round(strength * 100));
}

function GraphSvg({
  nodes,
  edges,
}: {
  nodes: { id: string; strength: number }[];
  edges: { id: string; weight: number }[];
}) {
  const layout = useMemo(() => {
    const n = nodes.length;
    if (n === 0) return [];
    const cx = 120;
    const cy = 90;
    const r = Math.min(70, 30 + n * 4);
    return nodes.map((node, i) => {
      const angle = (2 * Math.PI * i) / n - Math.PI / 2;
      return {
        ...node,
        x: cx + r * Math.cos(angle),
        y: cy + r * Math.sin(angle),
      };
    });
  }, [nodes]);

  const posById = useMemo(() => {
    const m = new Map<string, { x: number; y: number }>();
    for (const p of layout) {
      m.set(p.id, { x: p.x, y: p.y });
    }
    return m;
  }, [layout]);

  if (layout.length === 0) {
    return null;
  }

  return (
    <svg viewBox="0 0 240 180" className="w-full h-44 rounded-lg border border-card-border bg-canvas-alt">
      {edges.map((e) => {
        const parts = e.id.split('->');
        if (parts.length !== 2) return null;
        const a = posById.get(parts[0]);
        const b = posById.get(parts[1]);
        if (!a || !b) return null;
        const opacity = Math.min(1, 0.2 + e.weight * 0.6);
        return (
          <line
            key={e.id}
            x1={a.x}
            y1={a.y}
            x2={b.x}
            y2={b.y}
            stroke="currentColor"
            strokeOpacity={opacity}
            strokeWidth={1 + e.weight}
          />
        );
      })}
      {layout.map((node) => (
        <g key={node.id}>
          <circle
            cx={node.x}
            cy={node.y}
            r={6 + node.strength * 4}
            className="fill-accent/30 stroke-accent"
            strokeWidth={1}
          />
          <title>{node.id}</title>
        </g>
      ))}
    </svg>
  );
}

export default function TopicMemoryPanel({
  runtimeConn,
  streaming = false,
  runtimeSessionEstablished = false,
}: Props) {
  const { t } = useT();
  const runtimeReady = isRuntimeApiAvailable(runtimeConn, {
    streaming,
    sessionEstablished: runtimeSessionEstablished,
  });

  const [snapshot, setSnapshot] = useState<TopicMemorySnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    if (!runtimeReady) {
      setSnapshot(null);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const data = await fetchTopicMemory();
      setSnapshot(data);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [runtimeReady]);

  useEffect(() => {
    void refresh();
    if (!runtimeReady) return;
    const id = window.setInterval(() => void refresh(), 15000);
    return () => window.clearInterval(id);
  }, [refresh, runtimeReady]);

  const nodes = useMemo(() => {
    if (!snapshot?.graph?.nodes) return [];
    return Object.entries(snapshot.graph.nodes)
      .map(([id, n]) => ({ id, strength: n.strength ?? 0, count: n.count ?? 0 }))
      .sort((a, b) => b.strength - a.strength);
  }, [snapshot]);

  const edges = useMemo(() => {
    if (!snapshot?.graph?.edges) return [];
    return Object.entries(snapshot.graph.edges).map(([id, e]) => ({
      id,
      weight: e.weight ?? 0,
    }));
  }, [snapshot]);

  const metrics = snapshot?.metrics;

  return (
    <div className="overflow-y-auto px-3 py-3 space-y-3">
      <div className="flex items-center justify-between gap-2">
        <p className="text-xs text-t-text-muted">{t('topicMemoryPanel.hint')}</p>
        <button
          type="button"
          className="text-xs px-2 py-1 rounded border border-card-border hover:bg-hover"
          onClick={() => void refresh()}
          disabled={loading || !runtimeReady}
        >
          {loading ? t('common.loading') : t('common.refresh')}
        </button>
      </div>

      {!runtimeReady && (
        <p className="text-sm text-t-text-muted">{t('topicMemoryPanel.runtimeUnavailable')}</p>
      )}

      {error && <p className="text-sm text-danger">{error}</p>}

      {snapshot && !snapshot.enabled && (
        <p className="text-sm text-amber-text border border-amber-text/30 rounded-lg px-3 py-2">
          {t('topicMemoryPanel.disabled')}
        </p>
      )}

      {metrics && (
        <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
          <div className="rounded-lg border border-card-border bg-canvas-alt p-2 text-center">
            <div className="text-sm font-bold">{metrics.turn_updates}</div>
            <div className="text-[9px] text-t-text-muted">{t('topicMemoryPanel.turnUpdates')}</div>
          </div>
          <div className="rounded-lg border border-card-border bg-canvas-alt p-2 text-center">
            <div className="text-sm font-bold">{metrics.inject_count}</div>
            <div className="text-[9px] text-t-text-muted">{t('topicMemoryPanel.injectCount')}</div>
          </div>
          <div className="rounded-lg border border-card-border bg-canvas-alt p-2 text-center">
            <div className="text-sm font-bold">{(metrics.clarification_rate * 100).toFixed(1)}%</div>
            <div className="text-[9px] text-t-text-muted">{t('topicMemoryPanel.clarificationRate')}</div>
          </div>
          <div className="rounded-lg border border-card-border bg-canvas-alt p-2 text-center">
            <div className="text-sm font-bold">{nodes.length}</div>
            <div className="text-[9px] text-t-text-muted">{t('topicMemoryPanel.nodeCount')}</div>
          </div>
        </div>
      )}

      <GraphSvg
        nodes={nodes.map((n) => ({ id: n.id, strength: n.strength }))}
        edges={edges}
      />

      {nodes.length > 0 && (
        <section>
          <h3 className="text-xs font-semibold text-t-text-secondary mb-2">{t('topicMemoryPanel.topics')}</h3>
          <ul className="space-y-1.5">
            {nodes.slice(0, 24).map((node) => (
              <li key={node.id} className="flex items-center gap-2 text-xs">
                <span className="truncate flex-1 text-t-text">{node.id}</span>
                <span className="text-t-text-muted w-8 text-right">{node.count}</span>
                <div className="w-16 h-1.5 rounded bg-hover overflow-hidden">
                  <div
                    className="h-full bg-accent"
                    style={{ width: `${strengthPct(node.strength)}%` }}
                  />
                </div>
              </li>
            ))}
          </ul>
        </section>
      )}

      {snapshot?.graph_path && (
        <p className="text-[10px] text-t-text-muted break-all">{snapshot.graph_path}</p>
      )}
    </div>
  );
}

