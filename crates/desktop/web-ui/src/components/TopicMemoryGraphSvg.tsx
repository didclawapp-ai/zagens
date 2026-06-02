import { useCallback, useMemo, useRef, useState } from 'react';
import { useT } from '../i18n';
import type { TopicMemoryGraphEdgeInput, TopicMemoryGraphNodeInput } from '../lib/topicMemoryGraphLayout';
import {
  buildTopicMemoryLinks,
  layoutTopicMemoryGraph,
  linkLineEndpoints,
  neighborIds,
  nodeRadius,
  truncateTopicLabel,
} from '../lib/topicMemoryGraphLayout';

const VIEW_W = 360;
const VIEW_H = 260;
const MIN_ZOOM = 0.55;
const MAX_ZOOM = 2.4;

interface Props {
  nodes: TopicMemoryGraphNodeInput[];
  edges: TopicMemoryGraphEdgeInput[];
  selectedId?: string | null;
  onSelectNode?: (id: string | null) => void;
  emptyHint?: string;
}

export default function TopicMemoryGraphSvg({
  nodes,
  edges,
  selectedId = null,
  onSelectNode,
  emptyHint,
}: Props) {
  const { t } = useT();
  const [hoverId, setHoverId] = useState<string | null>(null);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [zoom, setZoom] = useState(1);
  const dragRef = useRef<{ x: number; y: number; panX: number; panY: number } | null>(null);

  const layout = useMemo(
    () => layoutTopicMemoryGraph(nodes, edges, VIEW_W, VIEW_H),
    [nodes, edges],
  );

  const posById = useMemo(() => {
    const m = new Map<string, { x: number; y: number }>();
    for (const p of layout) {
      m.set(p.id, { x: p.x, y: p.y });
    }
    return m;
  }, [layout]);

  const links = useMemo(
    () => buildTopicMemoryLinks(new Set(nodes.map((n) => n.id)), edges),
    [nodes, edges],
  );

  const maxWeight = useMemo(
    () => Math.max(0.5, ...links.map((l) => l.weight)),
    [links],
  );

  const focusId = hoverId ?? selectedId;
  const highlightIds = useMemo(() => {
    if (!focusId) return null;
    const n = new Set<string>([focusId]);
    for (const id of neighborIds(focusId, links)) {
      n.add(id);
    }
    return n;
  }, [focusId, links]);

  const onWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    const delta = e.deltaY > 0 ? 0.92 : 1.08;
    setZoom((z) => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, z * delta)));
  }, []);

  const onPointerDown = useCallback((e: React.PointerEvent<SVGSVGElement>) => {
    if (e.button !== 0) return;
    const target = e.target as Element;
    if (target.closest('[data-topic-node]')) return;
    dragRef.current = { x: e.clientX, y: e.clientY, panX: pan.x, panY: pan.y };
    (e.currentTarget as SVGSVGElement).setPointerCapture(e.pointerId);
  }, [pan.x, pan.y]);

  const onPointerMove = useCallback((e: React.PointerEvent<SVGSVGElement>) => {
    const d = dragRef.current;
    if (!d) return;
    const scale = zoom;
    setPan({
      x: d.panX + (e.clientX - d.x) / scale,
      y: d.panY + (e.clientY - d.y) / scale,
    });
  }, [zoom]);

  const onPointerUp = useCallback((e: React.PointerEvent<SVGSVGElement>) => {
    dragRef.current = null;
    try {
      (e.currentTarget as SVGSVGElement).releasePointerCapture(e.pointerId);
    } catch {
      /* already released */
    }
  }, []);

  const resetView = useCallback(() => {
    setPan({ x: 0, y: 0 });
    setZoom(1);
  }, []);

  if (layout.length === 0) {
    if (!emptyHint) return null;
    return (
      <p className="text-xs text-t-text-muted rounded-lg border border-card-border bg-canvas-alt px-3 py-6 text-center">
        {emptyHint}
      </p>
    );
  }

  return (
    <div className="relative rounded-lg border border-card-border bg-canvas-alt overflow-hidden">
      <svg
        viewBox={`0 0 ${VIEW_W} ${VIEW_H}`}
        className="w-full min-h-[13rem] touch-none cursor-grab active:cursor-grabbing text-accent"
        role="img"
        aria-hidden
        onWheel={onWheel}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerLeave={onPointerUp}
        onClick={(e) => {
          if ((e.target as Element).closest('[data-topic-node]')) return;
          onSelectNode?.(null);
        }}
      >
        <defs>
          <radialGradient id="topic-node-fill" cx="40%" cy="35%">
            <stop offset="0%" stopColor="currentColor" stopOpacity="0.45" />
            <stop offset="100%" stopColor="currentColor" stopOpacity="0.12" />
          </radialGradient>
        </defs>
        <g transform={`translate(${pan.x} ${pan.y}) scale(${zoom})`}>
          <rect
            x={0}
            y={0}
            width={VIEW_W}
            height={VIEW_H}
            fill="transparent"
            pointerEvents="all"
          />

          {links.map((link) => {
            const a = posById.get(link.source);
            const b = posById.get(link.target);
            if (!a || !b) return null;
            const nodeA = layout.find((n) => n.id === link.source);
            const nodeB = layout.find((n) => n.id === link.target);
            const rA = nodeA ? nodeRadius(nodeA.strength) : 10;
            const rB = nodeB ? nodeRadius(nodeB.strength) : 10;
            const { x1, y1, x2, y2 } = linkLineEndpoints(a.x, a.y, b.x, b.y, rA, rB);
            const norm = link.weight / maxWeight;
            const dimmed =
              highlightIds &&
              !highlightIds.has(link.source) &&
              !highlightIds.has(link.target);
            const opacity = dimmed ? 0.08 : 0.15 + norm * 0.55;
            const strokeWidth = dimmed ? 0.5 : 0.6 + norm * 2.2;
            const key = `${link.source}\u2192${link.target}`;
            return (
              <line
                key={key}
                x1={x1}
                y1={y1}
                x2={x2}
                y2={y2}
                stroke="currentColor"
                strokeOpacity={opacity}
                strokeWidth={strokeWidth}
                strokeLinecap="round"
              />
            );
          })}

          {layout.map((node) => {
            const r = nodeRadius(node.strength);
            const label = truncateTopicLabel(node.id);
            const dormant = node.dormant === true;
            const blocked = node.blocked === true;
            const dimmed = highlightIds && !highlightIds.has(node.id);
            const selected = selectedId === node.id;
            return (
              <g
                key={node.id}
                data-topic-node
                opacity={dimmed ? 0.25 : dormant ? 0.45 : 1}
                style={{ cursor: 'pointer' }}
                onPointerEnter={() => setHoverId(node.id)}
                onPointerLeave={() => setHoverId(null)}
                onClick={(e) => {
                  e.stopPropagation();
                  onSelectNode?.(selected ? null : node.id);
                }}
              >
                <circle
                  cx={node.x}
                  cy={node.y}
                  r={r}
                  fill="url(#topic-node-fill)"
                  stroke={blocked ? 'var(--color-danger, #f87171)' : 'currentColor'}
                  strokeOpacity={selected ? 1 : blocked ? 0.95 : 0.85}
                  strokeWidth={selected ? 2.4 : blocked ? 2 : 1.2}
                />
                <text
                  x={node.x}
                  y={node.y}
                  textAnchor="middle"
                  dominantBaseline="central"
                  className="fill-t-text pointer-events-none select-none"
                  fontSize={Math.max(7, Math.min(9, r * 0.85))}
                  opacity={0.95}
                >
                  {label}
                </text>
                <title>{node.id}</title>
              </g>
            );
          })}
        </g>
      </svg>
      {(pan.x !== 0 || pan.y !== 0 || zoom !== 1) && (
        <button
          type="button"
          className="absolute top-1 right-1 text-[10px] px-1.5 py-0.5 rounded border border-card-border bg-canvas/90 text-t-text-muted hover:bg-hover"
          onClick={resetView}
        >
          {t('topicMemoryPanel.resetView')}
        </button>
      )}
    </div>
  );
}
