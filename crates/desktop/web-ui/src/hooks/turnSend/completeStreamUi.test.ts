import { test } from 'vitest';
import assert from 'node:assert/strict';
import {
  mergeThreadTranscript,
  reconcileAssistantBlocks,
  reconcileMessagesFromThread,
} from '../../hooks/turnSend/completeStreamUi';
import type { TurnChatMessage } from '../../hooks/useTurnSend';
import type { TurnBlock } from '../../lib/chat/timeline/turnBlockTypes';

test('reconcileAssistantBlocks keeps live order and enriches tool output', () => {
  const live: TurnBlock[] = [
    { kind: 'thinking', id: 'th1', text: 'a', streaming: false, status: 'done' },
    { kind: 'tool', id: 't1', name: 'read_file', input: '{}', status: 'done' },
    { kind: 'text', id: 'tx1', content: 'hi', streaming: false },
  ];
  const persisted: TurnBlock[] = [
    { kind: 'thinking', id: 'th1', text: 'a longer', streaming: false, status: 'done' },
    {
      kind: 'tool',
      id: 't1',
      name: 'read_file',
      input: '{}',
      output: 'file body',
      status: 'done',
    },
    { kind: 'text', id: 'tx1', content: 'hi there', streaming: false },
  ];

  const merged = reconcileAssistantBlocks(live, { blocks: persisted });
  assert.equal(merged[0].kind, 'thinking');
  assert.equal(merged[0].kind === 'thinking' && merged[0].text, 'a longer');
  assert.equal(merged[1].kind === 'tool' && merged[1].output, 'file body');
  assert.equal(merged[2].kind === 'text' && merged[2].content, 'hi there');
});

test('reconcileAssistantBlocks does not duplicate equal-length final prose (thr_f6f6)', () => {
  const finalReport = [
    '协同白板项目构建完成，所有验证通过。',
    '',
    '## 技术选型理由',
    '',
    '**后端 Node.js (TypeScript)** — 前后端统一类型系统。',
    '',
    '## 已验证',
    '',
    '- 服务端编译零错误',
  ].join('\n');

  const live: TurnBlock[] = [
    { kind: 'text', id: 'c1', content: '我来先规划项目结构，然后逐步实现。', streaming: false },
    { kind: 'tool', id: 't1', name: 'write_file', input: '{}', status: 'done' },
    { kind: 'tool', id: 't2', name: 'exec_shell', input: '{}', status: 'done' },
    { kind: 'text', id: 'final', content: finalReport, streaming: false },
  ];
  // Persisted is richer (tool outputs) but final prose is the same length — must not push a 2nd copy.
  const persisted: TurnBlock[] = [
    { kind: 'text', id: 'c1', content: '我来先规划项目结构，然后逐步实现。', streaming: false },
    {
      kind: 'tool',
      id: 't1',
      name: 'write_file',
      input: '{}',
      output: 'wrote file',
      status: 'done',
    },
    {
      kind: 'tool',
      id: 't2',
      name: 'exec_shell',
      input: '{}',
      output: 'ok',
      status: 'done',
    },
    { kind: 'text', id: 'final-persisted', content: finalReport, streaming: false, itemId: 'item_final' },
  ];

  const merged = reconcileAssistantBlocks(live, { blocks: persisted });
  const texts = merged.filter((b) => b.kind === 'text');
  const finals = texts.filter((b) => b.kind === 'text' && b.content.includes('技术选型理由'));
  assert.equal(finals.length, 1, 'final report must appear once');
  assert.equal(merged.filter((b) => b.kind === 'tool' && b.output).length, 2);
});

test('reconcileAssistantBlocks matches final prose even when last live text is a short caption', () => {
  const finalReport = '协同白板项目构建完成。\n\n## 技术选型理由\n\n详情…'.repeat(3);
  const live: TurnBlock[] = [
    { kind: 'text', id: 'final', content: finalReport, streaming: false },
    { kind: 'tool', id: 't1', name: 'exec_shell', input: '{}', status: 'done' },
    { kind: 'text', id: 'cap', content: '全部验证通过。清理测试数据：', streaming: false },
  ];
  const persisted: TurnBlock[] = [
    { kind: 'text', id: 'final-p', content: finalReport, streaming: false },
    {
      kind: 'tool',
      id: 't1',
      name: 'exec_shell',
      input: '{}',
      output: 'cleaned',
      status: 'done',
    },
    { kind: 'text', id: 'cap-p', content: '全部验证通过。清理测试数据：', streaming: false },
  ];

  const merged = reconcileAssistantBlocks(live, { blocks: persisted });
  const finals = merged.filter(
    (b) => b.kind === 'text' && b.content.includes('技术选型理由'),
  );
  assert.equal(finals.length, 1);
});

test('reconcileAssistantBlocks does not re-push captions already inside coalesced live text', () => {
  // Live SSE often keeps several short captions in one text block; item-spine
  // replay emits one text per agent_message. Without a containment guard the
  // late sync would append those captions again as the turn grows.
  const coalesced =
    '结构清楚。现在并行定位问题源码。\n\n函数名不同。放宽搜索。\n\nH1 根因已定位：remove_shape_animations。';
  const live: TurnBlock[] = [
    { kind: 'text', id: 'live-tx', content: coalesced, streaming: false },
    {
      kind: 'tool',
      id: 't1',
      name: 'read_file',
      input: '{}',
      output: 'ok',
      status: 'done',
    },
  ];
  const persisted: TurnBlock[] = [
    { kind: 'text', id: 'p1', content: '结构清楚。现在并行定位问题源码。', streaming: false },
    { kind: 'text', id: 'p2', content: '函数名不同。放宽搜索。', streaming: false },
    {
      kind: 'text',
      id: 'p3',
      content: 'H1 根因已定位：remove_shape_animations。',
      streaming: false,
    },
    {
      kind: 'tool',
      id: 't1',
      name: 'read_file',
      input: '{}',
      output: 'ok',
      status: 'done',
    },
    { kind: 'text', id: 'p4', content: '继续定位 M2。', streaming: false },
  ];

  const merged = reconcileAssistantBlocks(live, { blocks: persisted });
  const texts = merged.filter((b) => b.kind === 'text');
  assert.equal(texts.length, 2, 'coalesced captions stay one block; only new prose is added');
  assert.equal(texts[0].kind === 'text' && texts[0].content, coalesced);
  assert.equal(texts[1].kind === 'text' && texts[1].content, '继续定位 M2。');
});

test('mergeThreadTranscript preserves thinkingIncomplete from persisted', () => {
  const live: TurnChatMessage[] = [
    { id: 'u1', role: 'user', content: 'go' },
    {
      id: 'a1',
      role: 'assistant',
      content: 'ok',
      isStreaming: true,
      blocks: [{ kind: 'text', id: 'tx', content: 'ok', streaming: true }],
    },
  ];
  const rebuilt: TurnChatMessage[] = [
    { id: 'u1', role: 'user', content: 'go' },
    {
      id: 'a1',
      role: 'assistant',
      content: 'ok',
      thinkingIncomplete: true,
      blocks: [
        { kind: 'tool', id: 't1', name: 'grep_files', input: '{}', status: 'done' },
        { kind: 'text', id: 'tx', content: 'ok', streaming: false },
      ],
    },
  ];

  const out = mergeThreadTranscript(live, rebuilt);
  const asst = out.find((m) => m.role === 'assistant');
  assert.equal(asst?.thinkingIncomplete, true);
  assert.ok((asst?.blocks?.length ?? 0) >= 2);
});

test('reconcileMessagesFromThread prefers richer persisted transcript length', () => {
  const live: TurnChatMessage[] = [
    { id: 'a1', role: 'assistant', content: 'partial', blocks: [] },
  ];
  const persisted: TurnChatMessage[] = [
    { id: 'u1', role: 'user', content: 'q' },
    {
      id: 'a1',
      role: 'assistant',
      content: 'full answer',
      blocks: [{ kind: 'text', id: 'tx', content: 'full answer', streaming: false }],
    },
  ];
  const out = reconcileMessagesFromThread(live, persisted);
  assert.equal(out[0].role, 'user');
  assert.equal(out[1].role, 'assistant');
  assert.equal(out[1].content, 'full answer');
});

test('reconcileMessagesFromThread does not replace streaming C with completed B (equal user count)', () => {
  // Race: user C is already on disk, but assistant C has no items yet so replay's
  // last assistant is still turn B. Live is mid-stream on C — merging must NOT
  // replace output C with output B (the "C → D replaced C" symptom).
  const live: TurnChatMessage[] = [
    { id: 'u1', role: 'user', content: '提示词A' },
    { id: 'a1', role: 'assistant', content: '输出A' },
    { id: 'u2', role: 'user', content: '提示词B' },
    {
      id: 'a2',
      role: 'assistant',
      content: '输出B完整长文',
      blocks: [{ kind: 'text', id: 'tb', content: '输出B完整长文', streaming: false }],
    },
    { id: 'u3', role: 'user', content: '提示词C' },
    {
      id: 'a3',
      role: 'assistant',
      content: '输出C正在流式',
      isStreaming: true,
      blocks: [{ kind: 'text', id: 'tc', content: '输出C正在流式', streaming: true }],
    },
  ];
  const persisted: TurnChatMessage[] = [
    { id: 'u1', role: 'user', content: '提示词A' },
    { id: 'a1', role: 'assistant', content: '输出A' },
    { id: 'u2', role: 'user', content: '提示词B' },
    {
      id: 'a2',
      role: 'assistant',
      content: '输出B完整长文',
      blocks: [{ kind: 'text', id: 'tb', content: '输出B完整长文', streaming: false }],
    },
    { id: 'u3', role: 'user', content: '提示词C' },
    // no assistant C yet
  ];

  const out = reconcileMessagesFromThread(live, persisted);
  assert.equal(out, live, 'must keep live when replay lacks the in-flight assistant');
  const a3 = out.find((m) => m.id === 'a3');
  assert.equal(a3?.content, '输出C正在流式');
  assert.equal(a3?.isStreaming, true);
  assert.ok(!a3?.content.includes('输出B'), 'turn B must not replace turn C');
});

test('reconcileMessagesFromThread skips stale snapshot that predates the in-flight turn', () => {
  // Turn 1 finished; user already sent prompt 2 and its assistant is streaming.
  const live: TurnChatMessage[] = [
    { id: 'u1', role: 'user', content: '第一次提示词' },
    {
      id: 'a1',
      role: 'assistant',
      content: '第一轮完整输出',
      blocks: [
        { kind: 'text', id: 't1', content: '第一轮完整输出', streaming: false },
        { kind: 'tool', id: 'tool1', name: 'exec_shell', input: '{}', output: 'ok', status: 'done' },
      ],
    },
    { id: 'u2', role: 'user', content: '第二次提示词' },
    {
      id: 'a2',
      role: 'assistant',
      content: '',
      isStreaming: true,
      blocks: [],
    },
  ];
  // Replay snapshot resolved late — it only knows about turn 1.
  const persisted: TurnChatMessage[] = [
    { id: 'pu1', role: 'user', content: '第一次提示词' },
    {
      id: 'pa1',
      role: 'assistant',
      content: '第一轮完整输出',
      blocks: [
        { kind: 'text', id: 'pt1', content: '第一轮完整输出', streaming: false },
        { kind: 'tool', id: 'tool1', name: 'exec_shell', input: '{}', output: 'ok', status: 'done' },
      ],
    },
  ];

  const out = reconcileMessagesFromThread(live, persisted);
  assert.equal(out, live, 'stale snapshot must not be merged');
  const a2 = out.find((m) => m.id === 'a2');
  assert.equal(a2?.blocks?.length, 0, 'turn-1 blocks must not be copied into the new streaming bubble');
  assert.equal(a2?.content, '', 'turn-1 prose must not duplicate into the new bubble');
  assert.equal(out.filter((m) => m.role === 'user').length, 2, 'in-flight user prompt kept');
});

test('reconcileMessagesFromThread finalizes stale running tools above a newer turn', () => {
  const live: TurnChatMessage[] = [
    {
      id: 'a-stale',
      role: 'assistant',
      content: '',
      blocks: [
        {
          kind: 'tool',
          id: 'call_01_bjxh',
          name: 'grep_files',
          input: '{}',
          status: 'running',
        },
      ],
    },
    { id: 'u2', role: 'user', content: '我对zagens-office引擎进行了更新' },
    {
      id: 'a-live',
      role: 'assistant',
      content: '',
      isStreaming: true,
      blocks: [
        {
          kind: 'tool',
          id: 'call_01_bjxh',
          name: 'grep_files',
          input: '{}',
          status: 'running',
        },
      ],
    },
  ];
  const persisted: TurnChatMessage[] = [
    {
      id: 'a-stale',
      role: 'assistant',
      content: '',
      blocks: [
        {
          kind: 'tool',
          id: 'call_01_bjxh',
          name: 'grep_files',
          input: '{}',
          status: 'running',
        },
      ],
    },
    { id: 'u2', role: 'user', content: '我对zagens-office引擎进行了更新' },
    {
      id: 'a-live',
      role: 'assistant',
      content: '',
      blocks: [
        {
          kind: 'tool',
          id: 'call_01_bjxh',
          name: 'grep_files',
          input: '{}',
          status: 'running',
        },
      ],
    },
  ];

  const out = reconcileMessagesFromThread(live, persisted);
  const stale = out.find((m) => m.id === 'a-stale');
  const liveAsst = out.find((m) => m.id === 'a-live');
  assert.equal(
    stale?.blocks?.find((b) => b.kind === 'tool' && b.id === 'call_01_bjxh')?.status,
    'interrupted',
  );
  assert.equal(
    liveAsst?.blocks?.find((b) => b.kind === 'tool' && b.id === 'call_01_bjxh')?.status,
    'running',
  );
});
