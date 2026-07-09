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
  // Leading completed thinking is absorbed into the explore activity (P4.6).
  assert.equal(items.length, 2);
  assert.equal(items[0].kind, 'collapsed_tools');
  if (items[0].kind === 'collapsed_tools') {
    assert.equal(items[0].blocks.length, 4);
    assert.equal(items[0].absorbedThinking?.length, 1);
  }
  assert.equal(items[1].kind, 'block');
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
  assert.equal(items.length, 2);
  assert.equal(items[0].kind, 'collapsed_tools');
  if (items[0].kind === 'collapsed_tools') {
    assert.equal(items[0].blocks.length, 3);
    assert.equal(items[0].category, 'explore');
  }
  assert.equal(items[1].kind, 'block');
  assert.equal(items[1].kind === 'block' && items[1].block.kind, 'text');
});

test('prepareTimelinePresentation collapses shell runs and leaves errors expanded', () => {
  const blocks: TurnBlock[] = [
    tool('s1', 'exec_shell'),
    tool('s2', 'exec_shell'),
    tool('s3', 'exec_shell'),
    { kind: 'tool', id: 's4', name: 'exec_shell', input: '{}', status: 'error', output: 'fail' },
  ];
  const items = prepareTimelinePresentation(blocks);
  assert.equal(items.length, 2);
  assert.equal(items[0].kind, 'collapsed_tools');
  if (items[0].kind === 'collapsed_tools') {
    assert.equal(items[0].category, 'shell');
    assert.equal(items[0].blocks.length, 3);
  }
  assert.equal(items[1].kind, 'block');
  assert.equal(items[1].kind === 'block' && items[1].block.status, 'error');
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
  assert.equal(items.length, 2);
  assert.equal(items[0].kind, 'collapsed_tools');
  if (items[0].kind === 'collapsed_tools') {
    assert.equal(items[0].category, 'shell');
    assert.equal(items[0].blocks.length, 3);
  }
  assert.equal(items[1].kind, 'collapsed_tools');
  if (items[1].kind === 'collapsed_tools') {
    assert.equal(items[1].category, 'plan');
    assert.equal(items[1].blocks.length, 3);
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
  assert.equal(items[1].kind, 'block');
});

test('prepareTimelinePresentation collapses office tool runs', () => {
  const blocks: TurnBlock[] = [
    tool('o1', 'read_office'),
    tool('o2', 'write_office'),
    tool('o3', 'load_office_payload'),
    { kind: 'text', id: 'x', content: '文档已生成。', streaming: false },
  ];
  const items = prepareTimelinePresentation(blocks);
  assert.equal(items.length, 2);
  assert.equal(items[0].kind, 'collapsed_tools');
  if (items[0].kind === 'collapsed_tools') {
    assert.equal(items[0].blocks.length, 3);
    assert.equal(items[0].category, 'office');
  }
  assert.equal(items[1].kind === 'block' && items[1].block.kind, 'text');
});

test('prepareTimelinePresentation merges office with explore across absorbed gaps', () => {
  const blocks: TurnBlock[] = [
    tool('r1', 'list_dir'),
    tool('r2', 'read_office'),
    { kind: 'thinking', id: 'th', text: 'draft next', streaming: false, status: 'done' },
    tool('w1', 'write_office'),
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
