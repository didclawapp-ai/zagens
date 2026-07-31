import { test } from 'vitest';
import assert from 'node:assert/strict';
import {
  buildTimelinePresentation,
  dedupeTimelineProseBlocks,
  prepareTimelinePresentation,
} from './timelineDisplayPipeline';
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
  // Leading completed thinking is absorbed into the explore activity (P4.6).
  assert.equal(items.length, 2);
  assert.equal(items[0].kind, 'collapsed_tools');
  if (items[0].kind === 'collapsed_tools') {
    assert.equal(items[0].blocks.length, 4);
    assert.equal(items[0].absorbedThinking?.length, 1);
  }
  assert.equal(items[1].kind, 'block');
});

test('prepareTimelinePresentation collapses file_info into explore activity', () => {
  const blocks: TurnBlock[] = [
    tool('a', 'file_info'),
    tool('b', 'file_info'),
    tool('c', 'read_file'),
    tool('d', 'file_info'),
    { kind: 'text', id: 'x', content: '核对完毕。', streaming: false },
  ];
  const items = prepareTimelinePresentation(blocks);
  assert.equal(items.length, 2);
  assert.equal(items[0].kind, 'collapsed_tools');
  if (items[0].kind === 'collapsed_tools') {
    assert.equal(items[0].blocks.length, 4);
    assert.equal(items[0].category, 'explore');
  }
  assert.equal(items[1].kind, 'block');
});

test('prepareTimelinePresentation collapses git/fetch explore tools with file_info', () => {
  const blocks: TurnBlock[] = [
    tool('g1', 'git_status'),
    tool('g2', 'git_diff'),
    tool('f1', 'fetch_url'),
    tool('i1', 'file_info'),
    { kind: 'text', id: 'x', content: '状态已核对。', streaming: false },
  ];
  const items = prepareTimelinePresentation(blocks);
  assert.equal(items.length, 2);
  assert.equal(items[0].kind, 'collapsed_tools');
  if (items[0].kind === 'collapsed_tools') {
    assert.equal(items[0].blocks.length, 4);
    assert.equal(items[0].category, 'explore');
  }
});

test('prepareTimelinePresentation collapses running tools into live activity', () => {
  const blocks: TurnBlock[] = [
    { kind: 'tool', id: 'r1', name: 'read_file', input: '{}', status: 'running' },
    tool('d1', 'read_file'),
    tool('d2', 'read_file'),
    tool('d3', 'read_file'),
  ];
  const items = prepareTimelinePresentation(blocks);
  assert.equal(items.length, 1);
  assert.equal(items[0].kind, 'collapsed_tools');
  if (items[0].kind === 'collapsed_tools') {
    assert.equal(items[0].blocks.length, 4);
    assert.equal(items[0].blocks[0].status, 'running');
  }
});

test('prepareTimelinePresentation merges running into adjacent done activity', () => {
  const blocks: TurnBlock[] = [
    { kind: 'tool', id: 'r1', name: 'scratchpad_set_area', input: '{}', status: 'running' },
    tool('d1', 'scratchpad_append'),
    tool('d2', 'scratchpad_status'),
  ];
  const items = prepareTimelinePresentation(blocks);
  assert.equal(items.length, 1);
  assert.equal(items[0].kind, 'collapsed_tools');
  if (items[0].kind === 'collapsed_tools') {
    assert.equal(items[0].blocks.length, 3);
  }
});

test('prepareTimelinePresentation absorbs short prose between explore tools', () => {
  const blocks: TurnBlock[] = [
    { kind: 'text', id: 'c1', content: 'Reading key files next.', streaming: false },
    tool('a', 'read_file'),
    { kind: 'text', id: 'c2', content: 'One more.', streaming: false },
    tool('b', 'grep_files'),
    tool('c', 'list_dir'),
    {
      kind: 'text',
      id: 'long',
      content:
        'Final report with a much longer body that must stay visible as its own text block after tools.',
      streaming: false,
    },
  ];
  const items = prepareTimelinePresentation(blocks);
  // Captions soft-split activities (thr_ea9c) while remaining as phase labels.
  assert.equal(items.length, 3);
  assert.equal(items[0].kind, 'collapsed_tools');
  if (items[0].kind === 'collapsed_tools') {
    assert.equal(items[0].blocks.length, 1);
    assert.equal(items[0].absorbedCaptions?.length, 1);
    assert.equal(items[0].absorbedCaptions?.[0]?.content, 'Reading key files next.');
  }
  assert.equal(items[1].kind, 'collapsed_tools');
  if (items[1].kind === 'collapsed_tools') {
    assert.equal(items[1].blocks.length, 2);
    assert.equal(items[1].absorbedCaptions?.[0]?.content, 'One more.');
  }
  assert.equal(items[2].kind, 'block');
  assert.equal(items[2].kind === 'block' && items[2].block.kind, 'text');
});

test('prepareTimelinePresentation merges failed shells into the same activity', () => {
  const blocks: TurnBlock[] = [
    tool('s1', 'exec_shell'),
    tool('s2', 'exec_shell'),
    tool('s3', 'exec_shell'),
    { kind: 'tool', id: 's4', name: 'exec_shell', input: '{}', status: 'error', output: 'fail' },
  ];
  const items = prepareTimelinePresentation(blocks);
  assert.equal(items.length, 1);
  assert.equal(items[0].kind, 'collapsed_tools');
  if (items[0].kind === 'collapsed_tools') {
    assert.equal(items[0].category, 'shell');
    assert.equal(items[0].blocks.length, 4);
    assert.equal(items[0].blocks.filter((b) => b.status === 'error').length, 1);
  }
});

test('prepareTimelinePresentation collapses consecutive failed shells and plan chips', () => {
  const blocks: TurnBlock[] = [
    { kind: 'tool', id: 'e1', name: 'exec_shell', input: '{}', status: 'error' },
    { kind: 'tool', id: 'e2', name: 'exec_shell', input: '{}', status: 'error' },
    { kind: 'tool', id: 'e3', name: 'exec_shell', input: '{}', status: 'error' },
    tool('p1', 'checklist_write'),
    tool('p2', 'checklist_update'),
    tool('p3', 'update_plan'),
  ];
  const items = prepareTimelinePresentation(blocks);
  // No caption between shell and plan → one mixed activity (thr_ea9c merge).
  assert.equal(items.length, 1);
  assert.equal(items[0].kind, 'collapsed_tools');
  if (items[0].kind === 'collapsed_tools') {
    assert.equal(items[0].category, 'mixed');
    assert.equal(items[0].blocks.length, 6);
  }
});

test('prepareTimelinePresentation absorbs mid-length planning asides before tools', () => {
  const aside =
    '没有网络访问 Go proxy。需要重构为纯标准库实现 — 手写 WebSocket 协议、JSON 文件存储、crypto/rand 生成 UUID。' +
    '同时把 gorilla/websocket 换成自研 wsutil，并把 google/uuid 换成 crypto/rand。';
  assert.ok(aside.length > 120 && aside.length <= 280, `aside len=${aside.length}`);
  const blocks: TurnBlock[] = [
    { kind: 'text', id: 'a1', content: aside, streaming: false },
    tool('w1', 'write_file'),
    tool('w2', 'edit_file'),
    {
      kind: 'text',
      id: 'final',
      content: '全部完成。\n\n## 项目结构\n\n' + 'x'.repeat(400),
      streaming: false,
    },
  ];
  const items = prepareTimelinePresentation(blocks);
  assert.equal(items.length, 2);
  assert.equal(items[0].kind, 'collapsed_tools');
  if (items[0].kind === 'collapsed_tools') {
    assert.equal(items[0].blocks.length, 2);
    assert.equal(items[0].absorbedCaptions?.length, 1);
  }
  assert.equal(items[1].kind, 'block');
  assert.equal(items[1].kind === 'block' && items[1].block.kind, 'text');
});

test('prepareTimelinePresentation bundles thinking↔shell alternation into one activity', () => {
  const blocks: TurnBlock[] = [
    { kind: 'thinking', id: 'th1', text: 'try build', streaming: false, status: 'done' },
    tool('s1', 'exec_shell'),
    { kind: 'thinking', id: 'th2', text: 'retry flags', streaming: false, status: 'done' },
    tool('s2', 'exec_shell'),
    { kind: 'thinking', id: 'th3', text: 'patch then rebuild', streaming: false, status: 'done' },
    tool('w1', 'edit_file'),
    tool('s3', 'exec_shell'),
    {
      kind: 'text',
      id: 'final',
      content: '全部完成。\n\n## 项目结构\n\n' + 'x'.repeat(400),
      streaming: false,
    },
  ];
  const items = prepareTimelinePresentation(blocks);
  assert.equal(items.length, 2, 'one activity + final prose');
  assert.equal(items[0].kind, 'collapsed_tools');
  if (items[0].kind === 'collapsed_tools') {
    assert.equal(items[0].blocks.length, 4);
    assert.equal(items[0].category, 'mixed');
    assert.equal(items[0].absorbedThinking?.length, 3);
  }
  assert.equal(items[1].kind === 'block' && items[1].block.kind, 'text');
});

test('prepareTimelinePresentation keeps streaming thinking visible', () => {
  const blocks: TurnBlock[] = [
    { kind: 'thinking', id: 'th', text: 'still…', streaming: true, status: 'running' },
    tool('s1', 'exec_shell'),
  ];
  const items = prepareTimelinePresentation(blocks);
  assert.equal(items.length, 2);
  assert.equal(items[0].kind === 'block' && items[0].block.kind, 'thinking');
  assert.equal(items[1].kind, 'collapsed_tools');
  if (items[1].kind === 'collapsed_tools') {
    assert.equal(items[1].blocks.length, 1);
    assert.equal(items[1].blocks[0].name, 'exec_shell');
  }
});

test('prepareTimelinePresentation collapses write tool runs', () => {
  const blocks: TurnBlock[] = [
    tool('w1', 'write_file'),
    tool('w2', 'edit_file'),
    tool('w3', 'apply_patch'),
    { kind: 'text', id: 'x', content: '文件已更新。', streaming: false },
  ];
  const items = prepareTimelinePresentation(blocks);
  assert.equal(items.length, 2);
  assert.equal(items[0].kind, 'collapsed_tools');
  if (items[0].kind === 'collapsed_tools') {
    assert.equal(items[0].blocks.length, 3);
    assert.equal(items[0].category, 'write');
  }
  assert.equal(items[1].kind === 'block' && items[1].block.kind, 'text');
});

test('prepareTimelinePresentation merges write with explore across absorbed gaps', () => {
  const blocks: TurnBlock[] = [
    tool('r1', 'list_dir'),
    tool('r2', 'read_file'),
    { kind: 'thinking', id: 'th', text: 'draft next', streaming: false, status: 'done' },
    tool('w1', 'write_file'),
    { kind: 'text', id: 'x', content: '完成。', streaming: false },
  ];
  const items = prepareTimelinePresentation(blocks);
  assert.equal(items.length, 2);
  assert.equal(items[0].kind, 'collapsed_tools');
  if (items[0].kind === 'collapsed_tools') {
    assert.equal(items[0].blocks.length, 3);
    assert.equal(items[0].category, 'mixed');
    assert.equal(items[0].absorbedThinking?.length, 1);
  }
});

test('prepareTimelinePresentation collapses load_skill and scratchpad workflow tools', () => {
  const blocks: TurnBlock[] = [
    tool('s1', 'load_skill'),
    tool('s2', 'scratchpad_init'),
    tool('s3', 'scratchpad_append'),
    { kind: 'text', id: 'x', content: '开始审核。', streaming: false },
  ];
  const items = prepareTimelinePresentation(blocks);
  assert.equal(items.length, 2);
  assert.equal(items[0].kind, 'collapsed_tools');
  if (items[0].kind === 'collapsed_tools') {
    assert.equal(items[0].blocks.length, 3);
    assert.equal(items[0].category, 'workflow');
  }
});

test('prepareTimelinePresentation collapses tool_search_tool_regex with workflow', () => {
  const blocks: TurnBlock[] = [
    tool('w1', 'scratchpad_set_area'),
    tool('ts', 'tool_search_tool_regex'),
    tool('w2', 'scratchpad_verify_note'),
    { kind: 'text', id: 'x', content: '继续。', streaming: false },
  ];
  const items = prepareTimelinePresentation(blocks);
  assert.equal(items.length, 2);
  assert.equal(items[0].kind, 'collapsed_tools');
  if (items[0].kind === 'collapsed_tools') {
    assert.equal(items[0].blocks.length, 3);
    assert.equal(items[0].category, 'workflow');
  }
});

test('prepareTimelinePresentation collapses agent_spawn sub-agent tools', () => {
  const blocks: TurnBlock[] = [
    tool('a1', 'agent_spawn'),
    tool('a2', 'agent_wait'),
    tool('a3', 'agent_result'),
    { kind: 'text', id: 'x', content: '子代理完成。', streaming: false },
  ];
  const items = prepareTimelinePresentation(blocks);
  assert.equal(items.length, 2);
  assert.equal(items[0].kind, 'collapsed_tools');
  if (items[0].kind === 'collapsed_tools') {
    assert.equal(items[0].blocks.length, 3);
    assert.equal(items[0].category, 'agent');
  }
});

test('prepareTimelinePresentation collapses lone workflow tools (thr_82ac)', () => {
  const blocks: TurnBlock[] = [
    tool('a', 'read_file'),
    tool('b', 'read_file'),
    tool('solo', 'scratchpad_set_area'),
    tool('c', 'grep_files'),
    tool('wf', 'load_skill'),
    tool('d', 'write_file'),
    { kind: 'text', id: 'x', content: '报告完成。', streaming: false },
  ];
  const items = prepareTimelinePresentation(blocks);
  assert.equal(items.length, 2, 'one activity + final prose');
  assert.equal(items[0].kind, 'collapsed_tools');
  if (items[0].kind === 'collapsed_tools') {
    assert.equal(items[0].blocks.length, 6);
    assert.equal(items[0].category, 'mixed');
  }
  assert.equal(items[1].kind === 'block' && items[1].block.kind, 'text');
});

test('dedupeTimelineProseBlocks collapses joined near-duplicate final reports', () => {
  const halfA =
    '协同白板应用已全部构建完成。以下是完整的设计说明和运行指南。\n\n## 技术选型\n\n' +
    '路径 server/store.go。端口 :300。\n' +
    '功能矩阵。'.repeat(30);
  const halfB =
    '协同白板应用已全部构建完成。以下是完整的设计说明和运行指南。\n\n## 技术选型\n\n' +
    '路径 server/store/store.go。端口 :3000。\n' +
    '功能矩阵。'.repeat(32);
  const blocks: TurnBlock[] = [
    { kind: 'text', id: 'dup', content: `${halfA}\n\n${halfB}`, streaming: false },
  ];
  const out = dedupeTimelineProseBlocks(blocks);
  assert.equal(out.length, 1);
  assert.ok(out[0].kind === 'text');
  assert.equal(out[0].content, halfB);
});

test('buildTimelinePresentation folds trailing thinking into final-report step', () => {
  const report =
    '协同白板项目已全部构建完成。以下是完整的设计说明和运行指南。\n\n## 技术选型\n\n' +
    '后端 Go + LevelDB。'.repeat(40);
  const blocks: TurnBlock[] = [
    { kind: 'text', id: 'cap', content: '开始构建。', streaming: false },
    tool('w1', 'write_file'),
    tool('w2', 'write_file'),
    tool('w3', 'write_file'),
    tool('s1', 'exec_shell'),
    tool('s2', 'exec_shell'),
    tool('s3', 'exec_shell'),
    { kind: 'text', id: 'report', content: report, streaming: false },
    { kind: 'thinking', id: 'th', text: '收尾。', streaming: false, status: 'done' },
  ];
  const roots = buildTimelinePresentation(blocks, { stepGrouping: true });
  const steps = roots.filter((r) => r.kind === 'step');
  assert.equal(steps.length, 2);
  assert.ok(steps.every((s) => s.kind === 'step' && s.title.trim()));
  const last = steps[1];
  assert.ok(last.kind === 'step');
  assert.ok(last.items.some((i) => i.kind === 'block' && i.block.kind === 'thinking'));
});

test('prepareTimelinePresentation thr_ea9c: shell fail/done zigzag + caption phases', () => {
  const blocks: TurnBlock[] = [
    { kind: 'text', id: 'c1', content: '安装依赖并验证编译。', streaming: false },
    tool('s1', 'exec_shell'),
    { kind: 'tool', id: 's2', name: 'exec_shell', input: '{}', status: 'error' },
    tool('s3', 'exec_shell'),
    { kind: 'tool', id: 's4', name: 'exec_shell', input: '{}', status: 'error' },
    tool('s5', 'exec_shell'),
    { kind: 'text', id: 'c2', content: '由于环境限制，改用纯 JS 转译。', streaming: false },
    tool('w1', 'write_file'),
    tool('w2', 'write_file'),
    { kind: 'tool', id: 's6', name: 'exec_shell', input: '{}', status: 'running' },
    {
      kind: 'text',
      id: 'report',
      content: '协同白板项目完成。以下是完整的架构总结和运行说明。\n\n## 项目结构\n\n' + 'x'.repeat(400),
      streaming: false,
    },
  ];
  const items = prepareTimelinePresentation(blocks);
  assert.equal(items.length, 3, 'two caption phases + final report');
  assert.equal(items[0].kind, 'collapsed_tools');
  if (items[0].kind === 'collapsed_tools') {
    assert.equal(items[0].blocks.length, 5);
    assert.equal(items[0].blocks.filter((b) => b.status === 'error').length, 2);
    assert.equal(items[0].absorbedCaptions?.[0]?.content, '安装依赖并验证编译。');
  }
  assert.equal(items[1].kind, 'collapsed_tools');
  if (items[1].kind === 'collapsed_tools') {
    assert.equal(items[1].blocks.length, 3);
    assert.equal(items[1].blocks.some((b) => b.status === 'running'), true);
    assert.equal(items[1].absorbedCaptions?.[0]?.content, '由于环境限制，改用纯 JS 转译。');
  }
  assert.equal(items[2].kind === 'block' && items[2].block.kind, 'text');
});
