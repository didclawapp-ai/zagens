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
  collectReconcileThreadIds,
  collectStreamingSessionIds,
  contextHasActiveStream,
  deleteContextFromMap,
  draftContextKey,
  ensureContextInMap,
  getViewMessagesFromMap,
  isActiveStreamView,
  isBackgroundStreamEvent,
  migrateDraftContextInMap,
  NEW_SESSION_DRAFT_KEY,
  patchContextInMap,
  removeThreadFromStreamingSet,
  resolveViewMessageKey,
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

assert.equal(
  isBackgroundStreamEvent(null, 'thr_b', null, true),
  false,
  'pending new-session send',
);
assert.equal(
  isBackgroundStreamEvent(null, 'thr_b', 'thr_b', true),
  false,
  'pending send after turn_started before activeThreadId catches up',
);
assert.equal(
  isBackgroundStreamEvent(null, 'thr_a', 'thr_a', false),
  true,
  'background thread while composer is on blank new session',
);
assert.equal(
  isBackgroundStreamEvent('thr_b', 'thr_a', 'thr_a', false),
  true,
  'background thread while viewing another session',
);
assert.equal(
  isBackgroundStreamEvent('thr_a', 'thr_a', 'thr_a', false),
  false,
  'active view thread',
);

const del = deleteContextFromMap(map, 'thr_a', 'thr_a');
assert.equal(del.deleted, true);
assert.equal(del.nextActiveThreadId, null, 'deleting active clears pointer');

const streaming = new Set(['thr_a', 'thr_b']);
assert.equal(removeThreadFromStreamingSet(streaming, 'thr_missing'), null);
const pruned = removeThreadFromStreamingSet(streaming, 'thr_a');
assert.deepEqual([...pruned!], ['thr_b']);

import {
  applyOptimisticThreadStop,
  applyThreadStatusEvent,
  detectThreadStatusDrift,
  getActiveThreadIdsFromStore,
  resetThreadStatusStoreForTests,
} from './threadStatusStore';

assert.deepEqual(collectReconcileThreadIds(null), []);
resetThreadStatusStoreForTests();
applyThreadStatusEvent({
  threadId: 'thr_a',
  status: 'streaming',
  seq: 1,
  source: 'test',
});
assert.deepEqual(collectReconcileThreadIds(null), ['thr_a']);
assert.deepEqual(collectReconcileThreadIds('thr_b').sort(), ['thr_a', 'thr_b']);
assert.deepEqual(collectReconcileThreadIds('thr_a'), ['thr_a']);
resetThreadStatusStoreForTests();

// ── SessionStrip streaming session ids (P3 store-driven) ─────────────────

resetThreadStatusStoreForTests();
const stripMap = new Map<string, StreamContext>();
ensureContextInMap(stripMap, 'thr_live', 'sess_live');
applyThreadStatusEvent({
  threadId: 'thr_live',
  status: 'streaming',
  seq: 1,
  source: 'test',
});

assert.equal(
  contextHasActiveStream(stripMap.get('thr_live')),
  false,
  'store is SSOT — registry message flags alone do not drive spinner',
);

const stripIds = collectStreamingSessionIds({
  activeThreadIds: getActiveThreadIdsFromStore(),
  contexts: stripMap,
  activeSessionId: 'sess_live',
  resumedThreadId: 'thr_live',
  activeThreadId: 'thr_live',
  pendingComposerStream: false,
});
assert.equal(
  stripIds.has('sess_live'),
  true,
  'sidebar spinner follows threadStatusStore active threads',
);

applyThreadStatusEvent({
  threadId: 'thr_live',
  status: 'idle',
  seq: 2,
  source: 'test',
});
const finishedStripIds = collectStreamingSessionIds({
  activeThreadIds: getActiveThreadIdsFromStore(),
  contexts: stripMap,
  activeSessionId: 'sess_live',
  resumedThreadId: 'thr_live',
  activeThreadId: 'thr_live',
  pendingComposerStream: false,
});
assert.equal(
  finishedStripIds.has('sess_live'),
  false,
  'strip spinner when store reports idle',
);

ensureContextInMap(stripMap, draftContextKey('sess_draft'), 'sess_draft');
patchContextInMap(stripMap, draftContextKey('sess_draft'), {
  messages: [{ id: 'a0', role: 'assistant', content: '', isStreaming: true }],
});
assert.equal(
  collectStreamingSessionIds({
    activeThreadIds: getActiveThreadIdsFromStore(),
    contexts: stripMap,
    activeSessionId: 'sess_draft',
    resumedThreadId: null,
    activeThreadId: null,
    pendingComposerStream: true,
  }).has('sess_draft'),
  true,
  'pending composer on draft bucket shows sidebar spinner',
);
resetThreadStatusStoreForTests();

assert.equal(makeEmptyPanelSlice().checklist, null);
assert.equal(makeEmptyContext('thr_x', null).threadId, 'thr_x');

// ── draft view message keys (registry SSOT) ─────────────────────────────

assert.equal(resolveViewMessageKey('thr_a', 'sess_1'), 'thr_a', 'thread wins over session');
assert.equal(resolveViewMessageKey(null, 'sess_1'), draftContextKey('sess_1'));
assert.equal(resolveViewMessageKey(null, null), NEW_SESSION_DRAFT_KEY);

const draftMap = new Map<string, StreamContext>();
ensureContextInMap(draftMap, NEW_SESSION_DRAFT_KEY, null);
patchContextInMap(draftMap, NEW_SESSION_DRAFT_KEY, {
  messages: [{ id: 'u1', role: 'user', content: 'draft' }],
});
assert.equal(
  getViewMessagesFromMap(draftMap, null, null).length,
  1,
  'new-session draft visible before turn_started',
);
assert.equal(
  migrateDraftContextInMap(draftMap, null, 'thr_new'),
  true,
  'draft migrates onto runtime thread',
);
assert.equal(draftMap.get(NEW_SESSION_DRAFT_KEY), undefined, 'draft bucket removed');
assert.equal(draftMap.get('thr_new')?.messages[0]?.content, 'draft');

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

// ── panelChannel thread-scoped dispatch (P0.6 hardening) ────────────────
//
// The dispatcher drops panel events whose `originThreadId` differs from the
// registered active thread. This is a defensive guard: the primary isolation
// is the `isBackground` early-return in `useTurnSend.applyNorm`, but this
// prevents future call sites that forget the check from leaking background
// panel state into the active UI.

import {
  dispatchPanelChecklist,
  dispatchPanelContext,
  dispatchPanelScratchpad,
  dispatchPanelTaskGraph,
  getPanelActiveThreadId,
  PANEL_CHECKLIST_EVENT,
  PANEL_CONTEXT_EVENT,
  PANEL_SCRATCHPAD_EVENT,
  PANEL_TASK_GRAPH_EVENT,
  setPanelActiveThreadId,
  shouldDispatchPanelForThread,
} from '../panelChannel';

// Minimal window/CustomEvent polyfill for the Node.js tsx runner.
const dispatched: Array<{ type: string; detail: unknown }> = [];
(globalThis as unknown as { window: unknown }).window = {
  dispatchEvent(ev: { type: string; detail: unknown }) {
    dispatched.push({ type: ev.type, detail: ev.detail });
  },
};
class FakeCustomEvent {
  type: string;
  detail: unknown;
  constructor(type: string, init?: { detail?: unknown }) {
    this.type = type;
    this.detail = init?.detail;
  }
}
(globalThis as unknown as { CustomEvent: typeof FakeCustomEvent }).CustomEvent =
  FakeCustomEvent;

function countDispatched(type: string): number {
  return dispatched.filter((d) => d.type === type).length;
}

// Reset state for a clean baseline.
setPanelActiveThreadId(null);
dispatched.length = 0;

// 1. No active thread registered → all events pass through (filter off).
dispatchPanelChecklist({ items: [], completion_pct: 0, in_progress_id: null }, 'thr_a');
assert.equal(countDispatched(PANEL_CHECKLIST_EVENT), 1, 'filter off: dispatch passes');

// 2. Register thr_a as active; thr_a events pass, thr_b events dropped.
setPanelActiveThreadId('thr_a');
dispatchPanelChecklist(null, 'thr_a');
assert.equal(countDispatched(PANEL_CHECKLIST_EVENT), 2, 'active thread event passes');
dispatchPanelChecklist(null, 'thr_b');
assert.equal(
  countDispatched(PANEL_CHECKLIST_EVENT),
  2,
  'non-active thread event dropped',
);

// 3. No originThreadId → always dispatch (backward-compatible, used by
//    sessionPanelReattach which restores a slice right after navigation).
dispatchPanelChecklist(null);
assert.equal(
  countDispatched(PANEL_CHECKLIST_EVENT),
  3,
  'no originThreadId → always dispatch',
);

// 4. All panel dispatchers enforce the same guard.
dispatchPanelScratchpad(null, 'thr_b');
assert.equal(countDispatched(PANEL_SCRATCHPAD_EVENT), 0, 'scratchpad dropped for non-active');
dispatchPanelTaskGraph({} as never, 'thr_a');
assert.equal(countDispatched(PANEL_TASK_GRAPH_EVENT), 1, 'task graph passes for active');
dispatchPanelContext({} as never, 'thr_b');
assert.equal(countDispatched(PANEL_CONTEXT_EVENT), 0, 'context dropped for non-active');

// 5. setPanelActiveThreadId(null) disables filtering again.
setPanelActiveThreadId(null);
dispatchPanelChecklist(null, 'thr_b');
assert.equal(
  countDispatched(PANEL_CHECKLIST_EVENT),
  4,
  'filter disabled after null → dispatch passes',
);

// 6. Whitespace is trimmed.
setPanelActiveThreadId('  thr_a  ');
assert.equal(getPanelActiveThreadId(), 'thr_a', 'active thread id trimmed');
dispatchPanelChecklist(null, 'thr_a');
assert.equal(countDispatched(PANEL_CHECKLIST_EVENT), 5, 'trimmed active matches');

// Restore module state for subsequent test runs in the same process.
setPanelActiveThreadId(null);

// 7. Agent panel reuses the same active-thread guard (S0.1 hardening).
setPanelActiveThreadId('thr_a');
assert.equal(
  shouldDispatchPanelForThread('thr_b'),
  false,
  'agent guard: non-active origin dropped',
);
assert.equal(
  shouldDispatchPanelForThread('thr_a'),
  true,
  'agent guard: active origin passes',
);
setPanelActiveThreadId(null);

// ── thread.status normalization (S2.1) ──────────────────────────────────

import { normalizeThreadStreamStatus } from './threadStatusStore';

assert.equal(normalizeThreadStreamStatus('streaming'), 'streaming');
assert.equal(normalizeThreadStreamStatus('awaiting_approval'), 'awaiting_approval');
assert.equal(normalizeThreadStreamStatus('idle'), 'idle');
assert.equal(normalizeThreadStreamStatus('bogus'), null);

// ── threadStatusStore (P3 authoritative) ───────────────────────────────

resetThreadStatusStoreForTests();
applyThreadStatusEvent({
  threadId: 'thr_store',
  status: 'streaming',
  seq: 10,
  source: 'test',
});
assert.equal(getActiveThreadIdsFromStore().has('thr_store'), true);
applyThreadStatusEvent({
  threadId: 'thr_store',
  status: 'streaming',
  seq: 9,
  source: 'test',
});
assert.equal(getActiveThreadIdsFromStore().has('thr_store'), true, 'stale seq ignored');
applyThreadStatusEvent({
  threadId: 'thr_store',
  status: 'idle',
  seq: 20,
  source: 'test',
});
assert.equal(getActiveThreadIdsFromStore().has('thr_store'), false);
const drifts = detectThreadStatusDrift(new Set(['thr_store']));
assert.equal(drifts.length, 1);
assert.equal(drifts[0]?.legacyInSet, true);
assert.equal(drifts[0]?.storeActive, false);
resetThreadStatusStoreForTests();

resetThreadStatusStoreForTests();
applyThreadStatusEvent({
  threadId: 'thr_stop',
  status: 'streaming',
  seq: 50,
  source: 'test',
});
applyOptimisticThreadStop('thr_stop', 'turn_x');
assert.equal(getActiveThreadIdsFromStore().has('thr_stop'), false);
applyThreadStatusEvent({
  threadId: 'thr_stop',
  status: 'streaming',
  seq: 51,
  source: 'test',
});
assert.equal(getActiveThreadIdsFromStore().has('thr_stop'), false, 'stale streaming after optimistic stop');
resetThreadStatusStoreForTests();

// ── idle context message eviction (S0.2) ───────────────────────────────

import {
  evictIdleContextMessages,
  IDLE_CONTEXT_EVICT_MS,
} from './streamContextAccess';
import type { StreamContextRegistry } from '../../hooks/useStreamContextRegistry';

const evictMap = new Map<string, StreamContext>();
ensureContextInMap(evictMap, 'thr_evict', 'sess_evict');
patchContextInMap(evictMap, 'thr_evict', {
  messages: [{ id: 'm1', role: 'user', content: 'hello' }],
  isStreaming: false,
  lastActivityAt: Date.now() - IDLE_CONTEXT_EVICT_MS - 1_000,
  threadTurn: { threadId: 'thr_evict', turnId: 'turn_1' },
});

const evictCache = new Map<string, { id: string; role: 'user' | 'assistant' | 'system'; content: string }[]>();
const evictRegistry = {
  getContext: (tid: string | null | undefined) => {
    const key = tid?.trim();
    return key ? evictMap.get(key) : undefined;
  },
  patchContext: (tid: string, patch: Partial<StreamContext>) => {
    patchContextInMap(evictMap, tid, patch);
  },
} as StreamContextRegistry;

assert.equal(
  evictIdleContextMessages(evictRegistry, 'thr_evict', 'thr_active', evictCache),
  true,
  'idle context evicted after threshold',
);
assert.equal(evictMap.get('thr_evict')?.messages.length, 0, 'messages cleared');
assert.equal(evictMap.get('thr_evict')?.threadTurn.turnId, 'turn_1', 'threadTurn kept');
assert.equal(evictMap.get('thr_evict')?.sessionId, 'sess_evict', 'sessionId kept');
assert.equal(evictCache.get('sess_evict')?.[0]?.content, 'hello', 'messages cached');

ensureContextInMap(evictMap, 'thr_active', 'sess_active');
patchContextInMap(evictMap, 'thr_active', {
  messages: [{ id: 'm2', role: 'user', content: 'active' }],
  isStreaming: false,
  lastActivityAt: Date.now() - IDLE_CONTEXT_EVICT_MS - 1_000,
});
assert.equal(
  evictIdleContextMessages(evictRegistry, 'thr_active', 'thr_active', evictCache),
  false,
  'active view context not evicted',
);

// ── Per-send stream key isolation (cross-talk fix) ──────────────────────
//
// Simulates the scenario: A (existing thread) is streaming, user opens a
// brand-new session B and sends. B's controller is stored under a per-send
// key (not a shared `__pending__`). When A completes and cleans up, B's
// controller must survive.

// Simulate two per-send keys (as generated by nextSendKey in useTurnSend).
const sendKeyA = '__send_1__';
const sendKeyB = '__send_2__';
const controllers = new Map<string, { aborted: boolean }>();

// A sends (has a real threadId, but we test the pending path too)
const ctrlA = { aborted: false };
controllers.set(sendKeyA, ctrlA);

// B sends (brand-new session, no threadId yet → per-send key)
const ctrlB = { aborted: false };
controllers.set(sendKeyB, ctrlB);

// A completes → only deletes ITS per-send key (not B's)
controllers.delete(sendKeyA);
assert.equal(controllers.has(sendKeyA), false, 'A key removed after A completes');
assert.equal(controllers.has(sendKeyB), true, 'B key survives A completion');
assert.equal(ctrlB.aborted, false, 'B controller not aborted by A completion');

// Simulate turn_started for B: migrate from per-send key to real threadId
const realThreadB = 'thr_b_real';
controllers.delete(sendKeyB);
controllers.set(realThreadB, ctrlB);
assert.equal(controllers.has(sendKeyB), false, 'B per-send key cleared after turn_started');
assert.equal(controllers.has(realThreadB), true, 'B controller migrated to real threadId');

// ── ownerSessionId closure isolation (cross-talk fix) ───────────────────
//
// Simulates: user on session S_A sends turn for thread A, then switches to
// session S_B before A's turn_started arrives. migrateDraftToThread must use
// S_A (captured at send time), not S_B (current active).

const ownerSessionId = 'sess_A'; // captured at send time
const activeSessionIdNow = 'sess_B'; // user switched after send

// Set up: A's draft in sess_A bucket, B's draft in sess_B bucket
const migrateMap = new Map<string, StreamContext>();
ensureContextInMap(migrateMap, draftContextKey(ownerSessionId), ownerSessionId);
patchContextInMap(migrateMap, draftContextKey(ownerSessionId), {
  messages: [{ id: 'u1', role: 'user', content: 'A draft' }],
});
ensureContextInMap(migrateMap, draftContextKey(activeSessionIdNow), activeSessionIdNow);
patchContextInMap(migrateMap, draftContextKey(activeSessionIdNow), {
  messages: [{ id: 'u2', role: 'user', content: 'B draft' }],
});

// Correct behavior: migrate with ownerSessionId (send-time session)
assert.equal(
  migrateDraftContextInMap(migrateMap, ownerSessionId, 'thr_A'),
  true,
  'A draft migrates with send-time session id',
);
assert.equal(migrateMap.get('thr_A')?.messages[0]?.content, 'A draft');

// Cross-talk prevention: B's draft is untouched (we used ownerSessionId, not
// activeSessionIdNow, so sess_B's bucket was never read)
assert.equal(
  migrateMap.get(draftContextKey(activeSessionIdNow))?.messages[0]?.content,
  'B draft',
  'B draft stays in its own bucket — no cross-talk',
);
assert.equal(
  migrateMap.get(draftContextKey(ownerSessionId)),
  undefined,
  'A draft bucket removed after migration',
);

console.log('multiSession.selfcheck: ok');
