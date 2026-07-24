import { test } from 'vitest';
import assert from 'node:assert/strict';
import {
  shouldAutoExpandActivityGroup,
  shouldPreferActivityExpanded,
  shouldUseLiveHoldPanel,
} from './collapsedToolRunExpand';

test('shouldAutoExpandActivityGroup expands only while tools are running', () => {
  assert.equal(
    shouldAutoExpandActivityGroup({ isTurnStreaming: true, runningCount: 1 }),
    true,
  );
  assert.equal(
    shouldAutoExpandActivityGroup({ isTurnStreaming: true, runningCount: 0 }),
    false,
  );
});

test('shouldPreferActivityExpanded keeps trailing row open during gaps', () => {
  assert.equal(
    shouldPreferActivityExpanded({
      isTurnStreaming: true,
      runningCount: 0,
      isTrailingActivity: true,
    }),
    true,
    'trailing activity stays open between tool events',
  );
  assert.equal(
    shouldPreferActivityExpanded({
      isTurnStreaming: true,
      runningCount: 0,
      isTrailingActivity: false,
    }),
    false,
    'earlier activities stay collapsed once superseded',
  );
  assert.equal(
    shouldPreferActivityExpanded({
      isTurnStreaming: false,
      runningCount: 0,
      isTrailingActivity: true,
    }),
    false,
    'collapse when the turn settles',
  );
  assert.equal(
    shouldPreferActivityExpanded({
      isTurnStreaming: true,
      runningCount: 2,
      isTrailingActivity: false,
    }),
    true,
  );
});

test('shouldUseLiveHoldPanel enables bounded live viewport for trailing activity', () => {
  assert.equal(
    shouldUseLiveHoldPanel({
      isTurnStreaming: true,
      isTrailingActivity: true,
    }),
    true,
  );
  assert.equal(
    shouldUseLiveHoldPanel({
      isTurnStreaming: true,
      isTrailingActivity: false,
    }),
    false,
  );
  assert.equal(
    shouldUseLiveHoldPanel({
      isTurnStreaming: false,
      isTrailingActivity: true,
    }),
    false,
  );
});
