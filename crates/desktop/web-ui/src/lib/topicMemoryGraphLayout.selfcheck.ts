import {
  buildTopicMemoryLinks,
  parseEdgeEndpoints,
  selectHotTopicSubgraph,
} from './topicMemoryGraphLayout';

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(msg);
}

const u = parseEdgeEndpoints('alpha\u2192beta');
assert(u?.[0] === 'alpha' && u?.[1] === 'beta', 'unicode arrow');

const legacy = parseEdgeEndpoints('a->b');
assert(legacy?.[0] === 'a' && legacy?.[1] === 'b', 'ascii arrow');

const links = buildTopicMemoryLinks(
  new Set(['x', 'y']),
  [{ id: 'x\u2192y', weight: 2 }],
);
assert(links.length === 1 && links[0].source === 'x', 'links');

const hot = selectHotTopicSubgraph(
  [
    { id: 'a', strength: 0.5 },
    { id: 'b', strength: 0.05, dormant: true },
    { id: 'c', strength: 0.2 },
  ],
  [
    { id: 'a\u2192c', weight: 3 },
    { id: 'a\u2192b', weight: 1 },
  ],
);
assert(hot.nodes.length === 2 && hot.edges.length === 1, 'hot subgraph');

console.log('topicMemoryGraphLayout.selfcheck: ok');
