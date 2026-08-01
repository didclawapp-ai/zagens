import { test } from 'vitest';
import assert from 'node:assert/strict';

import { applyRestoredChatMessages } from './applyRestoredChatMessages';
import type { TurnChatMessage } from '../../hooks/useTurnSend';

test('applyRestoredChatMessages keeps in-flight follow-up when replay arrives late', () => {
  const live: TurnChatMessage[] = [
    { id: 'u1', role: 'user', content: 'OK，继续测试DOCX引擎' },
    {
      id: 'a1',
      role: 'assistant',
      content: '上一轮已完成内容',
      blocks: [{ kind: 'text', id: 't1', content: '上一轮已完成内容', streaming: false }],
    },
    { id: 'u2', role: 'user', content: '引擎已经修复，重新确认测试' },
    {
      id: 'a2',
      role: 'assistant',
      content: '行动计划：\n1. 检查 CLI',
      isStreaming: true,
      blocks: [
        { kind: 'text', id: 't2', content: '行动计划：\n1. 检查 CLI', streaming: true },
      ],
    },
  ];
  const restored: TurnChatMessage[] = [
    { id: 'u1', role: 'user', content: 'OK，继续测试DOCX引擎' },
    {
      id: 'a1',
      role: 'assistant',
      content: '上一轮已完成内容',
      blocks: [{ kind: 'text', id: 't1', content: '上一轮已完成内容', streaming: false }],
    },
  ];

  const out = applyRestoredChatMessages(live, restored);
  assert.equal(out.filter((m) => m.role === 'user').length, 2, 'follow-up user kept');
  const a1 = out.find((m) => m.id === 'a1');
  const a2 = out.find((m) => m.id === 'a2');
  assert.equal(a1?.content, '上一轮已完成内容', 'prior turn not overwritten');
  assert.ok(!a1?.isStreaming, 'prior turn not left generating');
  assert.ok(a2?.content.includes('行动计划'), 'new stream kept');
  assert.equal(a2?.isStreaming, true);
});

test('applyRestoredChatMessages finalizes idle restore so no dual 生成中', () => {
  const restored: TurnChatMessage[] = [
    { id: 'u1', role: 'user', content: 'hi' },
    {
      id: 'a1',
      role: 'assistant',
      content: 'done',
      isStreaming: true,
      blocks: [
        { kind: 'text', id: 't1', content: 'done', streaming: true },
        {
          kind: 'tool',
          id: 'tool1',
          name: 'read_file',
          input: '{}',
          status: 'running',
        },
      ],
    },
  ];

  const out = applyRestoredChatMessages([], restored, { keepStreaming: false });
  const a1 = out.find((m) => m.id === 'a1');
  assert.equal(a1?.isStreaming, false);
  assert.equal(
    a1?.blocks?.find((b) => b.kind === 'tool')?.status,
    'interrupted',
  );
});
