import { test } from 'vitest';
import assert from 'node:assert/strict';
import { KNOWN_DESKTOP_SSE_EVENTS, normalizeDesktopStreamEvent } from './streamNormalize';

test('streamNormalize', () => {

assert.equal(
  normalizeDesktopStreamEvent({
    event: 'craft.spawned',
    data: JSON.stringify({ thread_id: 'thr_1' }),
  }),
  null,
  'unknown SSE events return null',
);

const completed = normalizeDesktopStreamEvent({
  event: 'turn.completed',
  data: JSON.stringify({ usage: { input_tokens: 1, output_tokens: 2 } }),
});
assert.equal(completed?.kind, 'turn_completed', 'turn.completed maps');

const segment = normalizeDesktopStreamEvent({
  event: 'message.segment',
  data: JSON.stringify({ content: '## Report\n\nDone.' }),
});
assert.equal(segment?.kind, 'message_segment', 'message.segment maps');
if (segment?.kind === 'message_segment') {
  assert.equal(segment.content, '## Report\n\nDone.');
}

const agentCompleted = normalizeDesktopStreamEvent({
  event: 'item.completed',
  data: JSON.stringify({
    event: 'item.completed',
    payload: {
      item: { kind: 'agent_message', detail: 'Final segment.' },
    },
  }),
});
assert.equal(agentCompleted?.kind, 'message_segment', 'item.completed agent_message maps to segment');

assert.equal(KNOWN_DESKTOP_SSE_EVENTS.has('thread.status'), true);

const threadStatus = normalizeDesktopStreamEvent({
  event: 'thread.status',
  data: JSON.stringify({
    thread_id: 'thr_1',
    turn_id: 'turn_1',
    status: 'idle',
    seq: 42,
  }),
  seq: 42,
});
assert.equal(threadStatus?.kind, 'thread_status');
if (threadStatus?.kind === 'thread_status') {
  assert.equal(threadStatus.threadId, 'thr_1');
  assert.equal(threadStatus.status, 'idle');
  assert.equal(threadStatus.seq, 42);
}

const rawThreadStatus = normalizeDesktopStreamEvent({
  event: 'thread.status',
  data: JSON.stringify({
    event_schema_version: 2,
    thread_id: 'thr_raw',
    turn_id: 'turn_raw',
    event: 'thread.status',
    payload: { status: 'streaming' },
  }),
});
assert.equal(rawThreadStatus?.kind, 'thread_status');
if (rawThreadStatus?.kind === 'thread_status') {
  assert.equal(rawThreadStatus.threadId, 'thr_raw');
  assert.equal(rawThreadStatus.status, 'streaming');
}
});
