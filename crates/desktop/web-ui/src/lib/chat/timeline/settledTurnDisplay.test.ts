import { test } from 'vitest';
import assert from 'node:assert/strict';
import {
  countToolsInPresentationItems,
  partitionFlatPresentationForSettledView,
  partitionPresentationForSettledView,
  stepHasVisibleProse,
} from './settledTurnDisplay';
import type {
  TimelinePresentationItem,
  TimelinePresentationRoot,
} from './timelinePresentationTypes';
import type { TurnBlock } from './turnBlockTypes';

function tool(id: string): Extract<TurnBlock, { kind: 'tool' }> {
  return { kind: 'tool', id, name: 'exec_shell', input: '{}', status: 'done' };
}

test('stepHasVisibleProse detects text blocks only', () => {
  const toolsOnly: TimelinePresentationItem[] = [
    {
      kind: 'collapsed_tools',
      id: 'c1',
      blocks: [tool('t1'), tool('t2')],
      category: 'shell',
    },
  ];
  assert.equal(stepHasVisibleProse(toolsOnly), false);

  const withText: TimelinePresentationItem[] = [
    ...toolsOnly,
    { kind: 'block', block: { kind: 'text', id: 'x', content: 'Done.', streaming: false } },
  ];
  assert.equal(stepHasVisibleProse(withText), true);
});

test('partitionFlatPresentationForSettledView keeps finals and bundles process', () => {
  const items: TimelinePresentationItem[] = [
    {
      kind: 'collapsed_tools',
      id: 'c1',
      blocks: [tool('t1'), tool('t2')],
      category: 'shell',
    },
    {
      kind: 'block',
      block: { kind: 'text', id: 'r1', content: 'Final report one.', streaming: false },
    },
    {
      kind: 'collapsed_tools',
      id: 'c2',
      blocks: [tool('t3')],
      category: 'write',
    },
    {
      kind: 'block',
      block: { kind: 'text', id: 'r2', content: 'Final report two.', streaming: false },
    },
  ];
  const segments = partitionFlatPresentationForSettledView(items);
  assert.equal(segments.length, 4);
  assert.equal(segments[0].kind, 'process');
  assert.equal(segments[1].kind, 'final');
  assert.equal(segments[2].kind, 'process');
  assert.equal(segments[3].kind, 'final');
  if (segments[0].kind === 'process') {
    assert.equal(countToolsInPresentationItems(segments[0].items), 2);
  }
});

test('partitionPresentationForSettledView merges tool-only steps (thr_ea9c)', () => {
  const report =
    '协同白板项目完成。以下是完整的架构总结。\n\n## 结构\n\n' + 'x'.repeat(300);
  const roots: TimelinePresentationRoot[] = [
    {
      kind: 'step',
      id: 's1',
      title: '开始搭建',
      stepIndex: 1,
      stepTotal: 4,
      items: [
        {
          kind: 'collapsed_tools',
          id: 'c1',
          blocks: [tool('t1'), tool('t2'), tool('t3')],
          category: 'mixed',
        },
      ],
    },
    {
      kind: 'step',
      id: 's2',
      title: '协同白板项目完成。',
      stepIndex: 2,
      stepTotal: 4,
      items: [
        { kind: 'block', block: { kind: 'text', id: 'r1', content: report, streaming: false } },
      ],
    },
    {
      kind: 'step',
      id: 's3',
      title: '加验证 Oracle',
      stepIndex: 3,
      stepTotal: 4,
      items: [
        {
          kind: 'collapsed_tools',
          id: 'c2',
          blocks: [tool('t4'), tool('t5')],
          category: 'shell',
        },
      ],
    },
    {
      kind: 'step',
      id: 's4',
      title: '验证通过。',
      stepIndex: 4,
      stepTotal: 4,
      items: [
        {
          kind: 'block',
          block: { kind: 'text', id: 'r2', content: '全部通过。\n\n' + 'y'.repeat(300), streaming: false },
        },
      ],
    },
  ];

  const segments = partitionPresentationForSettledView(roots);
  assert.equal(segments.length, 4);
  assert.equal(segments[0].kind, 'process');
  if (segments[0].kind === 'process') {
    assert.equal(segments[0].stepCount, 1);
    assert.equal(countToolsInPresentationItems(segments[0].items), 3);
  }
  assert.equal(segments[1].kind, 'final-step');
  assert.equal(segments[2].kind, 'process');
  if (segments[2].kind === 'process') {
    assert.equal(segments[2].stepCount, 1);
    assert.equal(countToolsInPresentationItems(segments[2].items), 2);
  }
  assert.equal(segments[3].kind, 'final-step');
});
