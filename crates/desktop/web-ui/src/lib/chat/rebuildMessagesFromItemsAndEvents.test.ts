import { test } from 'vitest';
import assert from 'node:assert/strict';
import { rebuildMessagesFromItemsAndEvents } from './rebuildMessagesFromThread';
import type { TurnItemRecord } from '../api/runtimeTypes';

const TURN_ID = 'turn_mix';

test('rebuildMessagesFromItemsAndEvents uses item spine and event thinking', () => {
  const items: TurnItemRecord[] = [
    {
      schema_version: 3,
      id: 'u1',
      turn_id: TURN_ID,
      kind: 'user_message',
      status: 'completed',
      summary: 'do it',
      artifact_refs: [],
    },
    {
      schema_version: 3,
      id: 'i1',
      turn_id: TURN_ID,
      kind: 'tool_call',
      status: 'completed',
      summary: 'ok',
      metadata: { tool: { id: 't1', name: 'read_file', input: { path: 'a.ts' } } },
      artifact_refs: [],
    },
    {
      schema_version: 3,
      id: 'i2',
      turn_id: TURN_ID,
      kind: 'agent_message',
      status: 'completed',
      summary: 'Done',
      artifact_refs: [],
    },
  ];
  const events = [
    {
      event: 'turn.started',
      data: JSON.stringify({ turn_id: TURN_ID, thread_id: 'thr' }),
    },
    {
      event: 'thinking.delta',
      data: JSON.stringify({ turn_id: TURN_ID, content: 'reason' }),
    },
    {
      event: 'tool.started',
      data: JSON.stringify({ turn_id: TURN_ID, id: 't1', name: 'read_file', input: '{}' }),
    },
    {
      event: 'tool.completed',
      data: JSON.stringify({ turn_id: TURN_ID, id: 't1', success: true, output: 'ok' }),
    },
    {
      event: 'message.segment',
      data: JSON.stringify({ turn_id: TURN_ID, content: 'Done' }),
    },
    {
      event: 'turn.completed',
      data: JSON.stringify({ turn_id: TURN_ID, usage: {} }),
    },
  ];

  const messages = rebuildMessagesFromItemsAndEvents(items, events);
  assert.equal(messages[0].role, 'user');
  assert.equal(messages[1].role, 'assistant');
  const blocks = messages[1].blocks ?? [];
  assert.ok(blocks.some((b) => b.kind === 'thinking'));
  assert.ok(blocks.some((b) => b.kind === 'tool'));
  assert.ok(blocks.some((b) => b.kind === 'text'));
  assert.equal(messages[1].thinkingIncomplete, undefined);
});

test('rebuildMessagesFromItemsAndEvents sets thinkingIncomplete without events', () => {
  const items: TurnItemRecord[] = [
    {
      schema_version: 3,
      id: 'i1',
      turn_id: TURN_ID,
      kind: 'agent_message',
      status: 'completed',
      summary: 'hello',
      artifact_refs: [],
    },
  ];
  const messages = rebuildMessagesFromItemsAndEvents(items, []);
  assert.equal(messages.length, 1);
  assert.equal(messages[0].thinkingIncomplete, true);
});
