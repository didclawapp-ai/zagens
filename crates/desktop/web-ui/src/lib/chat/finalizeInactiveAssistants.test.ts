import { test } from 'vitest';
import assert from 'node:assert/strict';

import {
  clearStreamingAssistantsUi,
  finalizeInactiveAssistants,
  markLastAssistantStreamingUi,
  rebindStreamingAssistantUi,
  type FinalizableChatMessage,
} from './finalizeInactiveAssistants';

function runningTool(id: string): NonNullable<FinalizableChatMessage['blocks']>[number] {
  return {
    kind: 'tool',
    id,
    name: 'grep_files',
    input: '{"path":"x"}',
    status: 'running',
  };
}

test('finalizeInactiveAssistants settles older assistants with running tools', () => {
  const messages: FinalizableChatMessage[] = [
    {
      id: 'a-stale',
      role: 'assistant',
      content: '',
      isStreaming: false,
      blocks: [runningTool('call_01_bjxh')],
    },
    { id: 'u1', role: 'user', content: '我对zagens-office引擎进行了更新' },
    {
      id: 'a-live',
      role: 'assistant',
      content: '',
      isStreaming: true,
      blocks: [runningTool('call_01_bjxh')],
    },
  ];

  const out = finalizeInactiveAssistants(messages, 'a-live');
  const stale = out.find((m) => m.id === 'a-stale');
  const live = out.find((m) => m.id === 'a-live');

  assert.equal(stale?.isStreaming, false);
  assert.equal(
    stale?.blocks?.find((b) => b.kind === 'tool' && b.id === 'call_01_bjxh')?.status,
    'interrupted',
  );
  assert.equal(live?.isStreaming, true);
  assert.equal(
    live?.blocks?.find((b) => b.kind === 'tool' && b.id === 'call_01_bjxh')?.status,
    'running',
  );
});

test('finalizeInactiveAssistants(null) settles every assistant before a new turn', () => {
  const messages: FinalizableChatMessage[] = [
    {
      id: 'a1',
      role: 'assistant',
      content: '',
      isStreaming: true,
      blocks: [runningTool('t1')],
    },
  ];
  const out = finalizeInactiveAssistants(messages, null);
  assert.equal(out[0].isStreaming, false);
  assert.equal(out[0].blocks?.[0].kind === 'tool' ? out[0].blocks[0].status : null, 'interrupted');
});

test('rebindStreamingAssistantUi finalizes non-target assistants', () => {
  const messages: FinalizableChatMessage[] = [
    {
      id: 'a-old',
      role: 'assistant',
      content: '',
      isStreaming: true,
      blocks: [runningTool('t-old')],
    },
    {
      id: 'a-new',
      role: 'assistant',
      content: '',
      isStreaming: false,
      blocks: [runningTool('t-new')],
    },
  ];
  const out = rebindStreamingAssistantUi(messages, 'a-new');
  assert.equal(out.find((m) => m.id === 'a-old')?.isStreaming, false);
  assert.equal(
    out.find((m) => m.id === 'a-old')?.blocks?.[0].kind === 'tool'
      ? out.find((m) => m.id === 'a-old')!.blocks![0].status
      : null,
    'interrupted',
  );
  assert.equal(out.find((m) => m.id === 'a-new')?.isStreaming, true);
  assert.equal(
    out.find((m) => m.id === 'a-new')?.blocks?.[0].kind === 'tool'
      ? out.find((m) => m.id === 'a-new')!.blocks![0].status
      : null,
    'running',
  );
});

test('clearStreamingAssistantsUi settles leftover running tools', () => {
  const messages: FinalizableChatMessage[] = [
    {
      id: 'a1',
      role: 'assistant',
      content: '',
      isStreaming: true,
      blocks: [runningTool('t1')],
    },
  ];
  const out = clearStreamingAssistantsUi(messages);
  assert.equal(out[0].isStreaming, false);
  assert.equal(out[0].blocks?.[0].kind === 'tool' ? out[0].blocks[0].status : null, 'interrupted');
});

test('markLastAssistantStreamingUi keeps only the last assistant live', () => {
  const messages: FinalizableChatMessage[] = [
    {
      id: 'a-old',
      role: 'assistant',
      content: '',
      blocks: [runningTool('t-old')],
    },
    {
      id: 'a-live',
      role: 'assistant',
      content: '',
      blocks: [runningTool('t-live')],
    },
  ];
  const { messages: out, assistantId } = markLastAssistantStreamingUi(messages);
  assert.equal(assistantId, 'a-live');
  assert.equal(out.find((m) => m.id === 'a-live')?.isStreaming, true);
  assert.equal(
    out.find((m) => m.id === 'a-old')?.blocks?.[0].kind === 'tool'
      ? out.find((m) => m.id === 'a-old')!.blocks![0].status
      : null,
    'interrupted',
  );
});
