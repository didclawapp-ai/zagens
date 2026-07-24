import { test } from 'vitest';
import assert from 'node:assert/strict';
import { trailingActivityIndex } from './activityPresentation';
import type { TimelinePresentationItem } from '../../../lib/chat/timeline/timelinePresentationTypes';

test('trailingActivityIndex finds last collapsed_tools row', () => {
  const items: TimelinePresentationItem[] = [
    { kind: 'block', block: { kind: 'thinking', id: 'th', text: 'x', streaming: true, status: 'running' } },
    {
      kind: 'collapsed_tools',
      id: 'c1',
      blocks: [{ kind: 'tool', id: 't1', name: 'exec_shell', input: '', status: 'done' }],
      category: 'shell',
    },
    {
      kind: 'collapsed_tools',
      id: 'c2',
      blocks: [{ kind: 'tool', id: 't2', name: 'write_file', input: '', status: 'running' }],
      category: 'write',
    },
  ];
  assert.equal(trailingActivityIndex(items), 2);
});
