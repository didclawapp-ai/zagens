import { test } from 'vitest';
import assert from 'node:assert/strict';

import {
  allocateMessageId,
  noteExistingMessageIds,
  resetMessageIdStateForTests,
} from './messageIds';

test('allocateMessageId never reuses restored msg-N ids', () => {
  resetMessageIdStateForTests();
  noteExistingMessageIds([
    { id: 'msg-1' },
    { id: 'msg-2' },
    { id: 'asst-1' },
    { id: 'item_abc' },
  ]);

  const a = allocateMessageId('msg');
  const b = allocateMessageId('msg');
  assert.notEqual(a, 'msg-1');
  assert.notEqual(a, 'msg-2');
  assert.notEqual(b, a);
  assert.match(a, /^msg-\d+-/);
  assert.match(b, /^msg-\d+-/);
});
