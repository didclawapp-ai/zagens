import { useCallback, useEffect, useMemo, useState } from 'react';
import type { RuntimeConnectionState } from '../api/client';
import {
  fetchTopicMemory,
  type TopicMemoryEmotion,
  type TopicMemorySnapshot,
} from '../api/client';
import { useT } from '../i18n';
import { isRuntimeApiAvailable } from '../lib/runtimeReachable';
import {
  formatAssociationLabel,
  selectHotTopicSubgraph,
} from '../lib/topicMemoryGraphLayout';
import TopicMemoryGraphSvg from './TopicMemoryGraphSvg';

interface Props {
  runtimeConn: RuntimeConnectionState;
  streaming?: boolean;
  runtimeSessionEstablished?: boolean;
}

function strengthPct(strength: number): number {
  return Math.min(100, Math.round(strength * 100));
}

function trailEmotionIcon(emotion: TopicMemoryEmotion): string {
  switch (emotion) {
    case 'A':
      return '⚡';
    case 'B':
      return '✨';
    case 'C':
      return '🌧';
    default:
      return '·';
  }
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
  const [selectedId, setSelectedId] = useState<string | null>(null);

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
      .map(([id, n]) => ({
        id,
        strength: n.strength ?? 0,
        count: n.count ?? 0,
        depth: n.depth,
        dormant: n.dormant,
        blocked: n.blocked,
      }))
      .sort((a, b) => b.strength - a.strength);
  }, [snapshot]);

  const edges = useMemo(() => {
    if (!snapshot?.graph?.edges) return [];
    return Object.entries(snapshot.graph.edges).map(([id, e]) => ({
      id,
      weight: e.weight ?? 0,
    }));
  }, [snapshot]);

  const graphNodes = useMemo(
    () =>
      nodes.map((n) => ({
        id: n.id,
        strength: n.strength,
        dormant: n.dormant,
        blocked: n.blocked,
        depth: n.depth,
      })),
    [nodes],
  );

  const hotSubgraph = useMemo(
    () => selectHotTopicSubgraph(graphNodes, edges),
    [graphNodes, edges],
  );

  const blockedPoints = useMemo(() => {
    const pts = snapshot?.graph?.blockedPoints ?? [];
    return [...pts].reverse().slice(0, 5);
  }, [snapshot]);

  const trails = useMemo(() => {
    const ts = snapshot?.graph?.trails ?? [];
    return [...ts].reverse().slice(0, 8);
  }, [snapshot]);

  const graphEmptyHint = useMemo(() => {
    if (hotSubgraph.nodes.length === 0) {
      return nodes.length > 0
        ? t('topicMemoryPanel.graphNoHotNodes')
        : t('topicMemoryPanel.graphEmpty');
    }
    return undefined;
  }, [hotSubgraph.nodes.length, nodes.length, t]);

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
        <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
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
          <div className="rounded-lg border border-card-border bg-canvas-alt p-2 text-center">
            <div className="text-sm font-bold">{(metrics.repeat_topic_rate * 100).toFixed(1)}%</div>
            <div className="text-[9px] text-t-text-muted">{t('topicMemoryPanel.repeatTopicRate')}</div>
          </div>
          <div className="rounded-lg border border-card-border bg-canvas-alt p-2 text-center">
            <div className="text-sm font-bold">{metrics.injects_per_10_turns.toFixed(1)}</div>
            <div className="text-[9px] text-t-text-muted">{t('topicMemoryPanel.injectsPer10')}</div>
          </div>
        </div>
      )}

      {nodes.length > 0 && (
        <p className="text-[10px] text-t-text-muted">{t('topicMemoryPanel.graphCaption')}</p>
      )}

      <TopicMemoryGraphSvg
        nodes={hotSubgraph.nodes}
        edges={hotSubgraph.edges}
        selectedId={selectedId}
        onSelectNode={setSelectedId}
        emptyHint={graphEmptyHint}
      />

      {hotSubgraph.nodes.length > 0 && hotSubgraph.edges.length === 0 && (
        <p className="text-xs text-t-text-muted -mt-1">{t('topicMemoryPanel.graphNoEdges')}</p>
      )}

      {hotSubgraph.edges.length > 0 && (
        <section>
          <h3 className="text-xs font-semibold text-t-text-secondary mb-2">
            {t('topicMemoryPanel.associations')}
          </h3>
          <ul className="space-y-1 text-xs text-t-text-muted">
            {hotSubgraph.edges.map((e) => (
              <li key={e.id} className="truncate">
                {formatAssociationLabel(e.id)}
                <span className="text-t-text-muted/80 ml-1">({e.weight.toFixed(1)})</span>
              </li>
            ))}
          </ul>
        </section>
      )}

      {nodes.length > 0 && (
        <section>
          <h3 className="text-xs font-semibold text-t-text-secondary mb-2">{t('topicMemoryPanel.topics')}</h3>
          <ul className="space-y-1.5">
            {nodes.slice(0, 24).map((node) => {
              const active = selectedId === node.id;
              return (
                <li key={node.id}>
                  <button
                    type="button"
                    className={`flex w-full items-center gap-2 text-xs rounded px-1 py-0.5 text-left ${
                      active ? 'bg-accent/15 ring-1 ring-accent/40' : 'hover:bg-hover'
                    }`}
                    onClick={() => setSelectedId(active ? null : node.id)}
                  >
                    <span className="truncate flex-1 text-t-text">{node.id}</span>
                    <span className="text-t-text-muted w-8 text-right">{node.count}</span>
                    <div className="w-16 h-1.5 rounded bg-hover overflow-hidden">
                      <div
                        className="h-full bg-accent"
                        style={{ width: `${strengthPct(node.strength)}%` }}
                      />
                    </div>
                  </button>
                </li>
              );
            })}
          </ul>
        </section>
      )}

      {blockedPoints.length > 0 && (
        <section>
          <h3 className="text-xs font-semibold text-t-text-secondary mb-2">
            {t('topicMemoryPanel.blocked')}
          </h3>
          <ul className="space-y-1.5 text-xs">
            {blockedPoints.map((b) => (
              <li key={`${b.node}-${b.since}`} className="text-t-text-muted">
                <span className="font-medium text-t-text">{b.node}</span>
                <span className="block truncate opacity-80">{b.context}</span>
              </li>
            ))}
          </ul>
        </section>
      )}

      {trails.length > 0 && (
        <section>
          <h3 className="text-xs font-semibold text-t-text-secondary mb-2">
            {t('topicMemoryPanel.trails')}
          </h3>
          <ul className="space-y-1 text-xs text-t-text-muted">
            {trails.map((tr, i) => (
              <li key={`${tr.date}-${tr.entry}-${i}`} className="truncate">
                <span className="mr-1">{trailEmotionIcon(tr.emotion)}</span>
                <span className="text-t-text">{tr.entry}</span>
                {' → '}
                <span className="text-t-text">{tr.exit}</span>
                <span className="opacity-70 ml-1">({tr.date})</span>
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
