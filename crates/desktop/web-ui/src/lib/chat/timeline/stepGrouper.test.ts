import { test } from 'vitest';
import assert from 'node:assert/strict';
import { groupPresentationIntoSteps, shortenStepTitle } from './stepGrouper';
import { prepareTimelinePresentation } from './timelineDisplayPipeline';
import type { TurnBlock } from './turnBlockTypes';

test('step group title prose is not rendered twice in body', () => {
  const caption = 'Now write the HTML and build config.';
  const longPhase =
    'Second phase with a much longer description that exceeds the caption threshold for grouping. ' +
    'It continues with enough detail about architecture, file layout, and verification steps so the ' +
    'step splitter treats this block as a major narrative boundary rather than a tool-run caption.';
  assert.ok(longPhase.length > 280, `longPhase len=${longPhase.length}`);
  const blocks: TurnBlock[] = [
    { kind: 'thinking', id: 'th1', text: 'plan', streaming: false, status: 'done' },
    { kind: 'text', id: 'tx1', content: caption, streaming: false },
    { kind: 'tool', id: 'tool1', name: 'write_file', input: '{}', status: 'done' },
    { kind: 'tool', id: 'plan1', name: 'checklist_update', input: '{}', status: 'done' },
    { kind: 'thinking', id: 'th2', text: 'more', streaming: false, status: 'done' },
    {
      kind: 'text',
      id: 'tx2',
      content: longPhase,
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

test('shortenStepTitle prefers heading and first sentence', () => {
  assert.equal(shortenStepTitle('## 审核结论\n\n很长的正文…'), '审核结论');
  assert.equal(
    shortenStepTitle('全库审核完成。报告已生成。\n\n## 审核结论\n\n' + 'x'.repeat(400)),
    '全库审核完成。',
  );
  const long =
    '报告已更新统计。现在我想把所有子代理发现的新增关键项整理为一份简洁的增量更新，而不是反复编辑已生成的MD文档。';
  const short = shortenStepTitle(long);
  assert.ok(short.length <= 72, short);
  assert.ok(short.startsWith('报告已更新统计'));
});

test('long final-report step keeps body and uses short title', () => {
  const report =
    '全库审核完成。报告已生成。\n\n## 审核结论\n\n**裁定: Request Changes** — ' +
    'x'.repeat(400);
  assert.ok(report.length > 280);
  const blocks: TurnBlock[] = [
    { kind: 'tool', id: 't1', name: 'write_file', input: '{}', status: 'done' },
    { kind: 'tool', id: 't2', name: 'checklist_write', input: '{}', status: 'done' },
    { kind: 'text', id: 'report', content: report, streaming: false },
  ];
  const roots = groupPresentationIntoSteps(blocks, prepareTimelinePresentation);
  const steps = roots.filter((r) => r.kind === 'step');
  assert.ok(steps.length >= 2);
  const reportStep = steps.find(
    (s) => s.kind === 'step' && s.items.some((i) => i.kind === 'block' && i.block.kind === 'text'),
  );
  assert.ok(reportStep && reportStep.kind === 'step');
  assert.ok(reportStep.title.length <= 72);
  assert.equal(reportStep.title.includes('x'.repeat(20)), false);
  const body = reportStep.items.find((i) => i.kind === 'block' && i.block.kind === 'text');
  assert.ok(body && body.kind === 'block' && body.block.kind === 'text');
  assert.equal(body.block.content, report);
});

test('trailing thinking after final report does not create empty step N/N', () => {
  const report =
    '协同白板应用已全部构建完成。以下是完整的设计说明和运行指南。\n\n## 技术选型\n\n' +
    '后端 Go + LevelDB。'.repeat(40);
  assert.ok(report.length > 280);
  const blocks: TurnBlock[] = [
    { kind: 'text', id: 'cap', content: '开始构建协同白板应用。', streaming: false },
    { kind: 'tool', id: 'w1', name: 'write_file', input: '{}', status: 'done' },
    { kind: 'tool', id: 'w2', name: 'write_file', input: '{}', status: 'done' },
    { kind: 'tool', id: 's1', name: 'exec_shell', input: '{}', status: 'done' },
    { kind: 'text', id: 'report', content: report, streaming: false },
    { kind: 'thinking', id: 'th-end', text: '收尾确认。', streaming: false, status: 'done' },
    { kind: 'thinking', id: 'th-end2', text: '再确认一次。', streaming: false, status: 'done' },
  ];
  const roots = groupPresentationIntoSteps(blocks, prepareTimelinePresentation);
  const steps = roots.filter((r) => r.kind === 'step');
  assert.equal(steps.length, 2, `expected 2 steps, got ${steps.length}`);
  assert.ok(steps.every((s) => s.kind === 'step' && s.title.trim().length > 0));
  const last = steps[1];
  assert.ok(last.kind === 'step');
  assert.ok(last.title.startsWith('协同白板应用已全部构建完成'));
  const thinkingInLast = last.items.filter(
    (i) => i.kind === 'block' && i.block.kind === 'thinking',
  );
  assert.equal(thinkingInLast.length, 2);
});

test('CJK first sentence title does not require space after period', () => {
  const title = shortenStepTitle(
    '协同白板应用已全部构建完成。以下是完整的设计说明和运行指南。 ## 技术选型',
  );
  assert.equal(title, '协同白板应用已全部构建完成。');
});
