import { test } from 'vitest';
import assert from 'node:assert/strict';
import {
  applyOptimisticThreadStop,
  getThreadStatusEntry,
  isThreadStreamActive,
  resetThreadStatusStoreForTests,
} from '../lib/chat/threadStatusStore';

test('optimistic stop marks thread idle so reconcile must not re-lock', () => {
  resetThreadStatusStoreForTests();
  applyOptimisticThreadStop('thr_wait', 'turn_1');
  const entry = getThreadStatusEntry('thr_wait');
  assert.ok(entry);
  assert.equal(entry?.status, 'idle');
  assert.equal(entry?.source, 'optimistic_stop');
  assert.equal(isThreadStreamActive(entry!.status), false);
});
