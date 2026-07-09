import { test } from 'vitest';
import assert from 'node:assert/strict';
import { consolidateProseWithTools } from './proseConsolidation';
import type { TurnBlock } from './turnBlockTypes';

test('consolidateProseWithTools pairs short text with following tool', () => {
  const blocks: TurnBlock[] = [
    { kind: 'text', id: 't1', content: 'Read the file first.', streaming: false },
    {
      kind: 'tool',
      id: 'tool1',
      name: 'read_file',
      input: '{}',
      status: 'done',
    },
  ];
  const out = consolidateProseWithTools(blocks);
  assert.equal(out.length, 1);
  assert.equal(out[0].kind, 'caption');
});
