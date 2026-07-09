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
