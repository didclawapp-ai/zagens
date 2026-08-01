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

test('applyBackgroundTimelineEvent does not overwrite a completed prior assistant', () => {
  const registry = makeRegistry();
  registry.ensureContext('thr_bg', 'sess_1');
  registry.patchContext('thr_bg', {
    messages: [
      { id: 'u1', role: 'user', content: '上一轮' },
      {
        id: 'a1',
        role: 'assistant',
        content: '上一轮已完成内容',
        isStreaming: false,
        blocks: [
          { kind: 'text', id: 't1', content: '上一轮已完成内容', streaming: false },
        ],
      },
    ],
  });

  applyBackgroundTimelineEvent(registry, 'thr_bg', 'sess_1', {
    kind: 'message_delta',
    content: '行动计划：新一轮',
  });

  const ctx = registry.getContext('thr_bg')!;
  assert.equal(ctx.messages.length, 3, 'user + prior assistant + new streaming row');
  assert.equal(ctx.messages[1].content, '上一轮已完成内容');
  assert.equal(ctx.messages[1].isStreaming, false);
  assert.equal(ctx.messages[2].isStreaming, true);
  assert.ok(String(ctx.messages[2].content).includes('行动计划'));
});
