import { test } from 'vitest';
import assert from 'node:assert/strict';
import { prepareTimelinePresentation } from './timelineDisplayPipeline';
import type { TurnBlock } from './turnBlockTypes';

function tool(id: string, name: string): Extract<TurnBlock, { kind: 'tool' }> {
  return {
    kind: 'tool',
    id,
    name,
    input: '{}',
    status: 'done',
  };
}

test('prepareTimelinePresentation collapses long explore runs', () => {
  const blocks: TurnBlock[] = [
    { kind: 'thinking', id: 't1', text: 'plan', streaming: false, status: 'done' },
    tool('a', 'read_file'),
    tool('b', 'read_file'),
    tool('c', 'grep_files'),
    tool('d', 'read_file'),
    { kind: 'text', id: 'x', content: 'done', streaming: false },
  ];
  const items = prepareTimelinePresentation(blocks);
  assert.equal(items.length, 3);
  assert.equal(items[0].kind, 'block');
  assert.equal(items[1].kind, 'collapsed_tools');
  if (items[1].kind === 'collapsed_tools') {
    assert.equal(items[1].blocks.length, 4);
  }
  assert.equal(items[2].kind, 'block');
});

test('prepareTimelinePresentation keeps running tools expanded', () => {
  const blocks: TurnBlock[] = [
    { kind: 'tool', id: 'r1', name: 'read_file', input: '{}', status: 'running' },
    tool('d1', 'read_file'),
    tool('d2', 'read_file'),
    tool('d3', 'read_file'),
  ];
  const items = prepareTimelinePresentation(blocks);
  assert.equal(items[0].kind, 'block');
  assert.equal(items[1].kind, 'collapsed_tools');
});
