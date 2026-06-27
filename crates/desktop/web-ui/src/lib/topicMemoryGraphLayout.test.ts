import { expect, test } from 'vitest';

import {
  buildTopicMemoryLinks,
  parseEdgeEndpoints,
  selectHotTopicSubgraph,
} from './topicMemoryGraphLayout';

test('topicMemoryGraphLayout', () => {
  const u = parseEdgeEndpoints('alpha\u2192beta');
  expect(u?.[0] === 'alpha' && u?.[1] === 'beta', 'unicode arrow').toBe(true);

  const legacy = parseEdgeEndpoints('a->b');
  expect(legacy?.[0] === 'a' && legacy?.[1] === 'b', 'ascii arrow').toBe(true);

  const links = buildTopicMemoryLinks(
    new Set(['x', 'y']),
    [{ id: 'x\u2192y', weight: 2 }],
  );
  expect(links.length === 1 && links[0].source === 'x', 'links').toBe(true);

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
  expect(hot.nodes.length).toBe(2);
  expect(hot.edges.length).toBe(2);
  expect(hot.edges[0]?.id).toBe('a\u2192c');
});
