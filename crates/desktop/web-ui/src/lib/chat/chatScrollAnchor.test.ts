import { test } from 'vitest';
import assert from 'node:assert/strict';
import {
  isStickToBottom,
  nextScrollTopAfterContentResize,
  scrollTopToPinElementTop,
} from './chatScrollAnchor';

test('isStickToBottom respects threshold', () => {
  assert.equal(isStickToBottom(1000, 880, 100, 120), true);
  assert.equal(isStickToBottom(1000, 700, 100, 120), false);
});

test('nextScrollTopAfterContentResize sticks to bottom on growth only', () => {
  assert.equal(
    nextScrollTopAfterContentResize({
      prevHeight: 800,
      newHeight: 1000,
      prevScrollTop: 700,
      clientHeight: 100,
      stickBottom: true,
    }),
    900,
  );
});

test('nextScrollTopAfterContentResize does not chase bottom on collapse', () => {
  assert.equal(
    nextScrollTopAfterContentResize({
      prevHeight: 1000,
      newHeight: 700,
      prevScrollTop: 400,
      clientHeight: 100,
      stickBottom: true,
    }),
    400,
    'shrink while stickBottom must not jump mid-list rows to the new end',
  );
});

test('scrollTopToPinElementTop restores header screen Y', () => {
  assert.equal(scrollTopToPinElementTop({ scrollTop: 500 }, 200, 120), 420);
});
