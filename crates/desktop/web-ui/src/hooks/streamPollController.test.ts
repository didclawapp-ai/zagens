import { test } from 'vitest';
import assert from 'node:assert/strict';
import { streamPollControllerAlive } from './useTurnStreamRecovery';

test('streamPollControllerAlive ignores missing or aborted controllers', () => {
  const map = new Map<string, AbortController>();
  assert.equal(streamPollControllerAlive(map, 'thr_a'), false);

  const dead = new AbortController();
  dead.abort();
  map.set('thr_a', dead);
  assert.equal(streamPollControllerAlive(map, 'thr_a'), false);

  map.set('thr_b', new AbortController());
  assert.equal(streamPollControllerAlive(map, 'thr_b'), true);
});
