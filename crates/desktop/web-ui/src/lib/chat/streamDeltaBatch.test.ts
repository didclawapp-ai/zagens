import { test } from 'vitest';
import assert from 'node:assert/strict';
import {
  STREAM_DELTA_BATCH_MS,
  createStreamDeltaBatcher,
} from './streamDeltaBatch';

test('createStreamDeltaBatcher coalesces deltas within the window', () => {
  const flushed: Array<{ kind: string; content: string }> = [];
  const timers: Array<{ id: number; fn: () => void }> = [];
  let nextId = 1;

  const batcher = createStreamDeltaBatcher(
    (kind, content) => {
      flushed.push({ kind, content });
    },
    {
      windowMs: STREAM_DELTA_BATCH_MS,
      schedule: (fn) => {
        const id = nextId++;
        timers.push({ id, fn });
        return id as unknown as ReturnType<typeof setTimeout>;
      },
      clearSchedule: (id) => {
        const idx = timers.findIndex((t) => t.id === (id as unknown as number));
        if (idx >= 0) timers.splice(idx, 1);
      },
    },
  );

  batcher.push('thinking_delta', 'a');
  batcher.push('thinking_delta', 'b');
  batcher.push('message_delta', 'x');
  batcher.push('message_delta', 'y');
  assert.equal(flushed.length, 0);
  assert.equal(timers.length, 1);

  timers[0].fn();
  assert.deepEqual(flushed, [
    { kind: 'thinking_delta', content: 'ab' },
    { kind: 'message_delta', content: 'xy' },
  ]);
});

test('createStreamDeltaBatcher flush preserves order before other events', () => {
  const flushed: string[] = [];
  const batcher = createStreamDeltaBatcher((kind, content) => {
    flushed.push(`${kind}:${content}`);
  });

  batcher.push('thinking_delta', 'think');
  batcher.push('message_delta', 'text');
  batcher.flush();
  assert.deepEqual(flushed, ['thinking_delta:think', 'message_delta:text']);

  batcher.flush();
  assert.equal(flushed.length, 2);
});
