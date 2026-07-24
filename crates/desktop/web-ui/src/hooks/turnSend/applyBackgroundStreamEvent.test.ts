import { test } from 'vitest';
import assert from 'node:assert/strict';
import {
  applyBackgroundTimelineEvent,
  isBackgroundTimelineContentEvent,
} from './applyBackgroundStreamEvent';
import { makeEmptyContext } from '../useStreamContextRegistry';
import type { StreamContextRegistry } from '../useStreamContextRegistry';

function makeRegistry(): StreamContextRegistry {
  const contexts = new Map<string, ReturnType<typeof makeEmptyContext>>();
  const contextsRef = { current: contexts };
  return {
    contexts,
    activeThreadId: null,
    setActiveThreadId: () => {},
    contextsRef,
    activeThreadIdRef: { current: null },
    getContext: (tid) => (tid ? contexts.get(tid) : undefined),
    ensureContext: (tid, sid) => {
      let ctx = contexts.get(tid);
      if (!ctx) {
        ctx = makeEmptyContext(tid, sid ?? null);
        contexts.set(tid, ctx);
      }
      return ctx;
    },
    patchContext: (tid, patch) => {
      const prev = contexts.get(tid);
      if (!prev) return;
      const next = typeof patch === 'function' ? patch(prev) : patch;
      contexts.set(tid, { ...prev, ...next });
    },
    migrateDraftToThread: () => {},
    deleteContext: () => {},
    getViewMessages: () => [],
    isActiveStreamView: () => false,
    version: 0,
  } as unknown as StreamContextRegistry;
}

test('isBackgroundTimelineContentEvent covers timeline deltas', () => {
  assert.equal(isBackgroundTimelineContentEvent('thinking_delta'), true);
  assert.equal(isBackgroundTimelineContentEvent('tool_started'), true);
  assert.equal(isBackgroundTimelineContentEvent('approval_required'), false);
});

test('applyBackgroundTimelineEvent accumulates thinking in registry', () => {
  const registry = makeRegistry();
  applyBackgroundTimelineEvent(registry, 'thr_bg', 'sess_1', {
    kind: 'thinking_delta',
    content: 'plan step',
  });
  const ctx = registry.getContext('thr_bg');
  assert.ok(ctx);
  assert.equal(ctx!.messages.length, 1);
  assert.equal(ctx!.messages[0].isStreaming, true);
  assert.equal(ctx!.timelineState?.blocks.length, 1);
  assert.equal(
    ctx!.timelineState?.blocks[0].kind === 'thinking' &&
      ctx!.timelineState?.blocks[0].text,
    'plan step',
  );
});
