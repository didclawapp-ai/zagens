/** Topic memory graph layout — edge keys use Unicode `→` per topic-memory engine. */

/** Align with `crates/topic-memory/src/engine.rs` injection caps. */
export const MAX_HOT_NODES = 12;
export const MAX_HOT_EDGES = 6;
export const MIN_HOT_NODE_STRENGTH = 0.1;

export interface TopicMemoryGraphNodeInput {
  id: string;
  strength: number;
  dormant?: boolean;
  blocked?: boolean;
  depth?: number;
}

export interface TopicMemoryGraphEdgeInput {
  id: string;
  weight: number;
}

export interface TopicMemoryLayoutNode extends TopicMemoryGraphNodeInput {
  x: number;
  y: number;
}

export interface TopicMemoryGraphLink {
  source: string;
  target: string;
  weight: number;
}

/** Parse `A→B` (engine default) or legacy `A->B`. */
export function parseEdgeEndpoints(edgeKey: string): [string, string] | null {
  const arrow = '\u2192';
  let sepIdx = edgeKey.indexOf(arrow);
  let sepLen = arrow.length;
  if (sepIdx < 0) {
    sepIdx = edgeKey.indexOf('->');
    sepLen = 2;
  }
  if (sepIdx < 0) return null;
  const a = edgeKey.slice(0, sepIdx).trim();
  const b = edgeKey.slice(sepIdx + sepLen).trim();
  if (!a || !b) return null;
  return [a, b];
}

/**
 * Subgraph for visualization — same hot-node / hot-edge policy as `generate_memory_section`:
 * - Top-N nodes by strength (above threshold, non-dormant)
 * - Top-M edges by weight globally (no both-ends constraint here; `buildTopicMemoryLinks`
 *   will skip edges whose endpoints are not on the canvas)
 */
export function selectHotTopicSubgraph(
  nodes: TopicMemoryGraphNodeInput[],
  edges: TopicMemoryGraphEdgeInput[],
): { nodes: TopicMemoryGraphNodeInput[]; edges: TopicMemoryGraphEdgeInput[] } {
  const hotNodes = nodes
    .filter((n) => !n.dormant && n.strength >= MIN_HOT_NODE_STRENGTH)
    .sort((a, b) => b.strength - a.strength)
    .slice(0, MAX_HOT_NODES);

  const hotEdges = edges
    .filter((e) => parseEdgeEndpoints(e.id) !== null)
    .sort((a, b) => b.weight - a.weight)
    .slice(0, MAX_HOT_EDGES);

  return { nodes: hotNodes, edges: hotEdges };
}

export function neighborIds(
  nodeId: string,
  links: TopicMemoryGraphLink[],
): Set<string> {
  const out = new Set<string>();
  for (const link of links) {
    if (link.source === nodeId) out.add(link.target);
    if (link.target === nodeId) out.add(link.source);
  }
  return out;
}

export function linkLineEndpoints(
  ax: number,
  ay: number,
  bx: number,
  by: number,
  rA: number,
  rB: number,
): { x1: number; y1: number; x2: number; y2: number } {
  const dx = bx - ax;
  const dy = by - ay;
  const dist = Math.hypot(dx, dy) || 1;
  const ux = dx / dist;
  const uy = dy / dist;
  return {
    x1: ax + ux * rA,
    y1: ay + uy * rA,
    x2: bx - ux * rB,
    y2: by - uy * rB,
  };
}

export function buildTopicMemoryLinks(
  nodeIds: Set<string>,
  edges: TopicMemoryGraphEdgeInput[],
): TopicMemoryGraphLink[] {
  const links: TopicMemoryGraphLink[] = [];
  for (const e of edges) {
    const ends = parseEdgeEndpoints(e.id);
    if (!ends) continue;
    const [source, target] = ends;
    if (!nodeIds.has(source) || !nodeIds.has(target)) continue;
    links.push({ source, target, weight: Math.max(0, e.weight) });
  }
  return links;
}

/** Stable circle seed: same ids → same positions across refreshes. */
function seedCircle(
  nodes: TopicMemoryGraphNodeInput[],
  width: number,
  height: number,
): { x: number; y: number }[] {
  const sorted = [...nodes].sort((a, b) => a.id.localeCompare(b.id));
  const n = sorted.length;
  const cx = width / 2;
  const cy = height / 2;
  const r = Math.min(width, height) * 0.32;
  const posById = new Map<string, { x: number; y: number }>();
  sorted.forEach((node, i) => {
    const angle = (2 * Math.PI * i) / n - Math.PI / 2;
    posById.set(node.id, {
      x: cx + r * Math.cos(angle),
      y: cy + r * Math.sin(angle),
    });
  });
  return nodes.map((node) => posById.get(node.id)!);
}

/**
 * Lightweight force-directed layout (no D3) for small topic graphs (≤ ~40 nodes).
 */
export function layoutTopicMemoryGraph(
  nodes: TopicMemoryGraphNodeInput[],
  edges: TopicMemoryGraphEdgeInput[],
  width: number,
  height: number,
): TopicMemoryLayoutNode[] {
  const n = nodes.length;
  if (n === 0) return [];

  const pad = 28;
  const nodeIds = new Set(nodes.map((nd) => nd.id));
  const links = buildTopicMemoryLinks(nodeIds, edges);
  const maxWeight = Math.max(1, ...links.map((l) => l.weight));

  const pos = seedCircle(nodes, width, height);
  const vel = pos.map(() => ({ x: 0, y: 0 }));
  const idToIdx = new Map(nodes.map((nd, i) => [nd.id, i]));

  const cx = width / 2;
  const cy = height / 2;
  const repulsion = 420;
  const linkPull = 0.045;
  const centerPull = 0.012;
  const damping = 0.82;
  const iterations = Math.min(160, 60 + n * 4);

  for (let tick = 0; tick < iterations; tick += 1) {
    const alpha = 1 - tick / iterations;
    const forces = pos.map(() => ({ x: 0, y: 0 }));

    for (let i = 0; i < n; i += 1) {
      for (let j = i + 1; j < n; j += 1) {
        const dx = pos[i].x - pos[j].x;
        const dy = pos[i].y - pos[j].y;
        const distSq = Math.max(dx * dx + dy * dy, 64);
        const dist = Math.sqrt(distSq);
        const f = (repulsion * alpha) / distSq;
        const fx = (dx / dist) * f;
        const fy = (dy / dist) * f;
        forces[i].x += fx;
        forces[i].y += fy;
        forces[j].x -= fx;
        forces[j].y -= fy;
      }
    }

    for (const link of links) {
      const si = idToIdx.get(link.source);
      const ti = idToIdx.get(link.target);
      if (si === undefined || ti === undefined) continue;
      const dx = pos[ti].x - pos[si].x;
      const dy = pos[ti].y - pos[si].y;
      const dist = Math.max(Math.hypot(dx, dy), 1);
      const desired = 36 + (1 - link.weight / maxWeight) * 48;
      const f = (dist - desired) * linkPull * link.weight * alpha;
      const fx = (dx / dist) * f;
      const fy = (dy / dist) * f;
      forces[si].x += fx;
      forces[si].y += fy;
      forces[ti].x -= fx;
      forces[ti].y -= fy;
    }

    for (let i = 0; i < n; i += 1) {
      forces[i].x += (cx - pos[i].x) * centerPull;
      forces[i].y += (cy - pos[i].y) * centerPull;
      vel[i].x = (vel[i].x + forces[i].x) * damping;
      vel[i].y = (vel[i].y + forces[i].y) * damping;
      pos[i].x = Math.min(width - pad, Math.max(pad, pos[i].x + vel[i].x));
      pos[i].y = Math.min(height - pad, Math.max(pad, pos[i].y + vel[i].y));
    }
  }

  return nodes.map((node, i) => ({
    ...node,
    x: pos[i].x,
    y: pos[i].y,
  }));
}

/** Short label for SVG (CJK / Latin). */
export function truncateTopicLabel(id: string, maxLen = 8): string {
  const t = id.trim();
  if (t.length <= maxLen) return t;
  return `${t.slice(0, maxLen)}…`;
}

/** Display `A→B` as `A → B`. */
export function formatAssociationLabel(edgeKey: string): string {
  const ends = parseEdgeEndpoints(edgeKey);
  if (!ends) return edgeKey;
  return `${ends[0]} → ${ends[1]}`;
}

export function nodeRadius(strength: number): number {
  return 10 + strength * 10;
}
