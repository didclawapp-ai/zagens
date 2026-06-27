import { test } from 'vitest';
import assert from 'node:assert/strict';

import {
  computeProgressScrollLayout,
  PROGRESS_ROW_H_PX,
  PROGRESS_ROW_STEP_PX,
  type ProgressScrollItem,
} from './progressScroll';

test('progressScroll', () => {
  const rows = (states: ProgressScrollItem['progress'][]): ProgressScrollItem[] =>
    states.map((progress, index) => ({ id: String(index), progress }));

  const layout = computeProgressScrollLayout(rows(['done', 'done', 'current', 'pending', 'pending']), 2);
  assert.equal(layout.openCount, 3);
  assert.equal(layout.focusIndex, 0);
  assert.equal(layout.offsetPx, 0);
  assert.equal(layout.overflow, true);
  assert.equal(layout.scrollTop, false);
  assert.equal(layout.scrollBottom, true);
  assert.equal(layout.viewportHeightPx, PROGRESS_ROW_STEP_PX * 2 - 3);

  const centered = computeProgressScrollLayout(rows(['done', 'pending', 'current', 'pending']), 2);
  assert.equal(centered.focusIndex, 1);
  assert.equal(centered.offsetPx, PROGRESS_ROW_STEP_PX);
  assert.equal(centered.scrollTop, true);
  assert.equal(centered.scrollBottom, false);

  const allDone = computeProgressScrollLayout(rows(['done', 'done']), 2);
  assert.equal(allDone.allDone, true);
  assert.equal(allDone.viewportHeightPx, PROGRESS_ROW_H_PX);

  const singleOpen = computeProgressScrollLayout(rows(['current']), 2);
  assert.equal(singleOpen.overflow, false);
  assert.equal(singleOpen.offsetPx, 0);
});
