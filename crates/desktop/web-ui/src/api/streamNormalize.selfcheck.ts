/**
 * A+.3 self-check for streamNormalize (run: npm run test:a+.3).
 * Unknown SSE event names must return null per API_DESIGN v1 forward-compat.
 */
import assert from 'node:assert/strict';

import { KNOWN_DESKTOP_SSE_EVENTS, normalizeDesktopStreamEvent } from './streamNormalize';

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

assert.equal(KNOWN_DESKTOP_SSE_EVENTS.has('turn.started'), true);
assert.equal(KNOWN_DESKTOP_SSE_EVENTS.has('craft.spawned'), false);

console.log('streamNormalize A+.3 self-check passed');
