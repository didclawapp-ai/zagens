import { test } from 'vitest';
import assert from 'node:assert/strict';
import { buildTimelinePresentation } from './timelineDisplayPipeline';
import type { TurnBlock } from './turnBlockTypes';

test('streamingTimelineDisplay collapses explore tool runs and groups steps', () => {
  const longProse =
    'Second phase with a much longer description that exceeds the caption threshold for grouping. ' +
    'It continues with enough detail about architecture, file layout, and verification steps so the ' +
    'step splitter treats this block as a major narrative boundary rather than a tool-run caption.';
  assert.ok(longProse.length > 280);
  const blocks: TurnBlock[] = [
    { kind: 'thinking', id: 'th', text: 'scout', streaming: false, status: 'done' },
    { kind: 'text', id: 'cap', content: 'Explore sources', streaming: false },
    {
      kind: 'tool',
      id: 't1',
      name: 'read_file',
      input: '{}',
      status: 'done',
      output: 'a',
    },
    {
      kind: 'tool',
      id: 't2',
      name: 'grep_files',
      input: '{}',
      status: 'done',
      output: 'b',
    },
    {
      kind: 'tool',
      id: 't3',
      name: 'glob_files',
      input: '{}',
      status: 'done',
      output: 'c',
    },
    {
      kind: 'tool',
      id: 't4',
      name: 'list_dir',
      input: '{}',
      status: 'done',
      output: 'd',
    },
    { kind: 'tool', id: 'plan1', name: 'checklist_update', input: '{}', status: 'done' },
    { kind: 'text', id: 'mid', content: longProse, streaming: false },
    {
      kind: 'tool',
      id: 't5',
      name: 'write_file',
      input: '{}',
      status: 'done',
      output: 'w',
    },
    { kind: 'text', id: 'final', content: 'All set.', streaming: false },
  ];

  const presentation = buildTimelinePresentation(blocks, { stepGrouping: true });
  assert.ok(presentation.length >= 1);

  const collapsed = presentation.flatMap((item) => {
    if ('kind' in item && item.kind === 'step') {
      return item.items.filter((i) => i.kind === 'collapsed_tools');
    }
    return item.kind === 'collapsed_tools' ? [item] : [];
  });
  assert.ok(collapsed.length >= 1, 'explore tools collapse into a run');

  const hasStep = presentation.some(
    (item) => typeof item === 'object' && item !== null && 'kind' in item && item.kind === 'step',
  );
  assert.equal(hasStep, true, 'long interleaved turn uses StepCard groups');
});
