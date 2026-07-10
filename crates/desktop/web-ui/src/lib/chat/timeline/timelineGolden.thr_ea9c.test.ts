/**
 * Regression golden for thr_ea9c-shaped turns:
 * high shell fail rate + mid captions + final reports + settled collapse.
 */
import { test } from 'vitest';
import assert from 'node:assert/strict';
import { buildTimelinePresentation } from './timelineDisplayPipeline';
import { partitionPresentationForSettledView } from './settledTurnDisplay';
import type { TurnBlock } from './turnBlockTypes';

function shell(id: string, status: 'done' | 'error' | 'running' = 'done'): TurnBlock {
  return { kind: 'tool', id, name: 'exec_shell', input: '{}', status };
}

function write(id: string): TurnBlock {
  return { kind: 'tool', id, name: 'write_file', input: '{}', status: 'done' };
}

test('thr_ea9c golden: activity merge + caption phases + settled finals only', () => {
  const report1 =
    '协同白板项目完成。以下是完整的架构总结和运行说明。\n\n## 项目结构\n\n' +
    'x'.repeat(400);
  const report2 =
    '所有 4 项验证 oracle 均已实跑通过。\n\n## 验证结果\n\n' + 'y'.repeat(400);

  const blocks: TurnBlock[] = [
    {
      kind: 'tool',
      id: 'cl',
      name: 'checklist_write',
      input: JSON.stringify({
        todos: [
          { content: '创建项目目录结构', status: 'in_progress' },
          { content: '实现 CRDT', status: 'pending' },
        ],
      }),
      status: 'done',
    },
    { kind: 'text', id: 'c1', content: '开始搭建协同白板项目。', streaming: false },
    write('w1'),
    write('w2'),
    write('w3'),
    { kind: 'text', id: 'c2', content: '安装依赖并验证编译。', streaming: false },
    shell('s1'),
    shell('s2', 'error'),
    shell('s3'),
    shell('s4', 'error'),
    shell('s5', 'running'),
    { kind: 'text', id: 'r1', content: report1, streaming: false },
    { kind: 'text', id: 'c3', content: '加验证 Oracle，逐项实跑：', streaming: false },
    shell('s6'),
    shell('s7', 'error'),
    write('w4'),
    { kind: 'text', id: 'r2', content: report2, streaming: false },
  ];

  const live = buildTimelinePresentation(blocks, { stepGrouping: true });
  assert.ok(live.length >= 2, 'step grouping produces multiple roots');

  const settled = partitionPresentationForSettledView(live);
  const finals = settled.filter(
    (s) => s.kind === 'final-step' || s.kind === 'final-item',
  );
  const process = settled.filter((s) => s.kind === 'process');

  assert.ok(finals.length >= 2, 'both final reports stay visible');
  assert.ok(process.length >= 1, 'tool phases fold into process bundles');

  // No zigzag: each process segment is one collapsed activity trail, not N error rows.
  for (const seg of process) {
    if (seg.kind !== 'process') continue;
    assert.ok(seg.items.length >= 1);
    const activities = seg.items.filter((i) => i.kind === 'collapsed_tools');
    assert.ok(activities.length >= 1);
    for (const act of activities) {
      if (act.kind !== 'collapsed_tools') continue;
      const errors = act.blocks.filter((b) => b.status === 'error').length;
      if (errors > 0) {
        assert.ok(
          act.blocks.length > errors,
          'failed shells share a bucket with successes when present',
        );
      }
    }
  }

  const firstFinal = finals[0];
  assert.ok(firstFinal.kind === 'final-step');
  assert.ok(firstFinal.step.title.startsWith('协同白板项目完成'));
});
