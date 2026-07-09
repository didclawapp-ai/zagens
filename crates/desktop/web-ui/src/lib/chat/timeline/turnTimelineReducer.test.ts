import { test } from 'vitest';
import assert from 'node:assert/strict';
import {
  applyTimelineEvent,
  createEmptyTimelineState,
} from './turnTimelineReducer.ts';

test('applyTimelineEvent interleaves thinking, tool, and text', () => {
  let state = createEmptyTimelineState();
  state = applyTimelineEvent(state, { kind: 'thinking_delta', content: 'plan' });
  state = applyTimelineEvent(state, {
    kind: 'tool_started',
    id: 't1',
    name: 'read_file',
    input: { path: 'a.ts' },
  });
  state = applyTimelineEvent(state, { kind: 'thinking_delta', content: 'next' });
  state = applyTimelineEvent(state, {
    kind: 'tool_completed',
    id: 't1',
    success: true,
    output: 'ok',
  });
  state = applyTimelineEvent(state, { kind: 'message_delta', content: 'done' });

  assert.deepEqual(
    state.blocks.map((b) => b.kind),
    ['thinking', 'tool', 'thinking', 'text'],
  );
});

test('message_segment skips duplicate prose already shown before trailing tools', () => {
  let state = createEmptyTimelineState();
  state = applyTimelineEvent(state, { kind: 'thinking_delta', content: 'hmm' });
  state = applyTimelineEvent(state, {
    kind: 'message_delta',
    content: 'Now write the WebSocket layer.',
  });
  state = applyTimelineEvent(state, {
    kind: 'tool_started',
    id: 't1',
    name: 'write_file',
    input: '{}',
  });
  state = applyTimelineEvent(state, {
    kind: 'tool_completed',
    id: 't1',
    success: true,
    output: 'ok',
  });
  const before = state.blocks.length;
  state = applyTimelineEvent(state, {
    kind: 'message_segment',
    content: 'Now write the WebSocket layer.',
  });
  assert.equal(state.blocks.length, before);
  assert.equal(
    state.blocks.filter((b) => b.kind === 'text').length,
    1,
  );
});
