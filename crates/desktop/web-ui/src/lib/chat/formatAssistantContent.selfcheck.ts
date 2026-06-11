import assert from 'node:assert/strict';

import {
  enhanceAssistantParagraphBreaks,
  mergeAgentMessageSegment,
} from './formatAssistantContent';
import { rebuildMessagesFromEventRecords } from './rebuildMessagesFromThread';

assert.equal(
  enhanceAssistantParagraphBreaks('已完成。**P1-T01** 新建 crate'),
  '已完成。\n\n**P1-T01** 新建 crate',
  'inserts break before task marker',
);

assert.equal(
  mergeAgentMessageSegment('开始实施。', '继续读取参考文件。'),
  '开始实施。\n\n继续读取参考文件。',
  'merges turn segments with paragraph gap',
);

assert.equal(
  mergeAgentMessageSegment('hello', 'hello'),
  'hello',
  'skips duplicate completed segment',
);

const turnEvents = [
  {
    event: 'item.completed',
    data: JSON.stringify({
      event: 'item.completed',
      payload: {
        item: { kind: 'user_message', detail: '请实施' },
      },
    }),
  },
  {
    event: 'item.delta',
    data: JSON.stringify({
      payload: { kind: 'agent_message', delta: '开始实施。' },
    }),
  },
  {
    event: 'item.completed',
    data: JSON.stringify({
      event: 'item.completed',
      payload: {
        item: { kind: 'agent_message', detail: '开始实施。' },
      },
    }),
  },
  {
    event: 'item.started',
    data: JSON.stringify({
      payload: {
        tool: { id: 't1', name: 'read_file', input: { path: 'lib.rs' } },
      },
    }),
  },
  {
    event: 'item.completed',
    data: JSON.stringify({
      event: 'item.completed',
      payload: {
        tool: { id: 't1', name: 'read_file' },
        item: { kind: 'tool_call', id: 't1', detail: 'ok' },
      },
    }),
  },
  {
    event: 'item.delta',
    data: JSON.stringify({
      payload: { kind: 'agent_message', delta: '继续读取。' },
    }),
  },
  {
    event: 'item.completed',
    data: JSON.stringify({
      event: 'item.completed',
      payload: {
        item: { kind: 'agent_message', detail: '继续读取。' },
      },
    }),
  },
  {
    event: 'turn.completed',
    data: JSON.stringify({
      event: 'turn.completed',
      payload: { turn: { usage: { input_tokens: 1, output_tokens: 2 } } },
    }),
  },
];

const rebuilt = rebuildMessagesFromEventRecords(turnEvents);
assert.equal(rebuilt.length, 2, 'one user bubble + one assistant bubble per turn');
assert.equal(rebuilt[0].role, 'user');
assert.equal(rebuilt[1].role, 'assistant');
assert.equal(
  rebuilt[1].content,
  '开始实施。\n\n继续读取。',
  'assistant segments merge into one bubble with paragraph breaks',
);
assert.equal(rebuilt[1].tools?.length, 1, 'tool cards stay in the same assistant bubble');

console.log('formatAssistantContent self-check passed');
