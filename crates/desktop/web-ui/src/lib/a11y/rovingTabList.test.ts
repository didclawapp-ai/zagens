import { test } from 'vitest';
import assert from 'node:assert/strict';
import { nextTabListIndex } from './rovingTabList';

test('rovingTabList', () => {

assert.equal(nextTabListIndex('ArrowRight', 0, 3), 1);
assert.equal(nextTabListIndex('ArrowLeft', 0, 3), 2);
assert.equal(nextTabListIndex('ArrowDown', 2, 3), 0);
assert.equal(nextTabListIndex('ArrowUp', 1, 3), 0);
assert.equal(nextTabListIndex('Home', 2, 3), 0);
assert.equal(nextTabListIndex('End', 0, 3), 2);
assert.equal(nextTabListIndex('Tab', 0, 3), null);
assert.equal(nextTabListIndex('ArrowRight', 0, 0), null);
});
