/**
 * Multi-session parallel streaming self-checks (run: npm run test:multi-session).
 * Covers pure helpers used by the P0/P1 desktop parallel-stream path.
 */
import assert from 'node:assert/strict';

import {
  lastAssistantMessageId,
  markLastAssistantStreaming,
  rebindStreamingAssistant,
  resolveStreamTargetId,
} from './activeTurnStreamUi';
import {
  SESSIONS_VISIBLE_PER_DAY,
  formatSessionDateKey,
  groupSessionsByDate,
} from './sessionStripGrouping';
import {
  deleteContextFromMap,
  ensureContextInMap,
  isActiveStreamView,
  patchContextInMap,
  removeThreadFromStreamingSet,
} from './streamContextStore';
import {
  makeEmptyContext,
  makeEmptyPanelSlice,
  type StreamContext,
} from '../../hooks/useStreamContextRegistry';

// ── sessionStripGrouping ────────────────────────────────────────────────

assert.equal(SESSIONS_VISIBLE_PER_DAY, 5);

const dayA = Date.UTC(2026, 5, 19, 10, 0, 0);
const dayB = Date.UTC(2026, 5, 18, 15, 0, 0);

assert.equal(formatSessionDateKey(dayA), '2026/06/19');

const grouped = groupSessionsByDate([
  { id: 's1', name: 'older same day', updated_at: dayA - 3_600_000 },
  { id: 's2', name: 'newer same day', updated_at: dayA },
  { id: 's3', name: 'yesterday', updated_at: dayB },
]);
assert.equal(grouped.length, 2);
assert.equal(grouped[0].dateKey, '2026/06/19');
assert.equal(grouped[0].sessions[0].id, 's2', 'newest session first within day');
assert.equal(grouped[1].dateKey, '2026/06/18');

// ── streamContextStore ──────────────────────────────────────────────────

const map = new Map<string, StreamContext>();
const first = ensureContextInMap(map, 'thr_a', 'sess_1');
assert.equal(first.changed, true);
assert.equal(first.ctx.sessionId, 'sess_1');
assert.equal(first.ctx.isStreaming, false);

const second = ensureContextInMap(map, 'thr_a', 'sess_1');
assert.equal(second.changed, false, 'same session id is a no-op');

const updated = ensureContextInMap(map, 'thr_a', 'sess_2');
assert.equal(updated.changed, true);
assert.equal(updated.ctx.sessionId, 'sess_2');

assert.equal(
  patchContextInMap(map, 'thr_missing', { isStreaming: true }),
  false,
  'patch missing thread is a no-op',
);
assert.equal(
  patchContextInMap(map, 'thr_a', { isStreaming: true }),
  true,
  'patch existing thread succeeds',
);
assert.equal(map.get('thr_a')?.isStreaming, true);

assert.equal(isActiveStreamView('thr_a', 'thr_a'), true);
assert.equal(isActiveStreamView('thr_a', 'thr_b'), false);
assert.equal(isActiveStreamView(null, 'thr_b'), false);
assert.equal(isActiveStreamView('thr_a', null), true, 'threadless events pass through');

const del = deleteContextFromMap(map, 'thr_a', 'thr_a');
assert.equal(del.deleted, true);
assert.equal(del.nextActiveThreadId, null, 'deleting active clears pointer');

const streaming = new Set(['thr_a', 'thr_b']);
assert.equal(removeThreadFromStreamingSet(streaming, 'thr_missing'), null);
const pruned = removeThreadFromStreamingSet(streaming, 'thr_a');
assert.deepEqual([...pruned!], ['thr_b']);

assert.equal(makeEmptyPanelSlice().checklist, null);
assert.equal(makeEmptyContext('thr_x', null).threadId, 'thr_x');

// ── activeTurnStreamUi / resolveStreamTargetId ──────────────────────────

const messages = [
  { id: 'u1', role: 'user' as const, content: 'hi' },
  { id: 'a1', role: 'assistant' as const, content: 'hello', isStreaming: false },
  { id: 'a2', role: 'assistant' as const, content: '…', isStreaming: true },
];

assert.equal(lastAssistantMessageId(messages), 'a2');

const target = { assistantId: 'stale-id' };
assert.equal(
  resolveStreamTargetId(messages, target),
  'a2',
  'rebinds to last assistant when streamTarget id is stale',
);
assert.equal(target.assistantId, 'a2');

const rebound = rebindStreamingAssistant(messages, 'a1');
assert.equal(rebound.find((m) => m.id === 'a2')?.isStreaming, false);
assert.equal(rebound.find((m) => m.id === 'a1')?.isStreaming, true);

const marked = markLastAssistantStreaming([
  { id: 'a-old', role: 'assistant', content: 'done', isStreaming: false },
  { id: 'a-live', role: 'assistant', content: '…', isStreaming: false },
]);
assert.equal(marked.assistantId, 'a-live');
assert.equal(marked.messages.find((m) => m.id === 'a-live')?.isStreaming, true);

console.log('multiSession.selfcheck: ok');
