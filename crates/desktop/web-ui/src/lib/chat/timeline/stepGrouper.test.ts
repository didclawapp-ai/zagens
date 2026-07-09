import { test } from 'vitest';
import assert from 'node:assert/strict';
import { groupPresentationIntoSteps } from './stepGrouper';
import { prepareTimelinePresentation } from './timelineDisplayPipeline';
import type { TurnBlock } from './turnBlockTypes';

test('step group title prose is not rendered twice in body', () => {
  const caption = 'Now write the HTML and build config.';
  const blocks: TurnBlock[] = [
    { kind: 'thinking', id: 'th1', text: 'plan', streaming: false, status: 'done' },
    { kind: 'text', id: 'tx1', content: caption, streaming: false },
    { kind: 'tool', id: 'tool1', name: 'write_file', input: '{}', status: 'done' },
    { kind: 'tool', id: 'plan1', name: 'checklist_update', input: '{}', status: 'done' },
    { kind: 'thinking', id: 'th2', text: 'more', streaming: false, status: 'done' },
    {
      kind: 'text',
      id: 'tx2',
      content:
        'Second phase with a much longer description that exceeds the caption threshold for grouping.',
      streaming: false,
    },
    { kind: 'tool', id: 'tool2', name: 'read_file', input: '{}', status: 'done' },
  ];

  const roots = groupPresentationIntoSteps(blocks, prepareTimelinePresentation);
  const steps = roots.filter((r) => r.kind === 'step');
  assert.ok(steps.length >= 2);
  const titled = steps.find((s) => s.kind === 'step' && s.title === caption);
  assert.ok(titled && titled.kind === 'step');
  const bodyText = titled.items
    .filter((i) => i.kind === 'block' && i.block.kind === 'text')
    .map((i) => (i.kind === 'block' && i.block.kind === 'text' ? i.block.content : ''));
  assert.equal(bodyText.includes(caption), false);
});
