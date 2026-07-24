import { test } from 'vitest';
import assert from 'node:assert/strict';
import {
  applyStreamEventToMessages,
  isTimelineStreamEvent,
} from './applyStreamEventToMessages';
import { createEmptyTimelineState } from '../../lib/chat/timeline/turnTimelineReducer';
import type { TurnChatMessage } from '../useTurnSend';

test('isTimelineStreamEvent covers interleaved kinds', () => {
  assert.equal(isTimelineStreamEvent('thinking_delta'), true);
  assert.equal(isTimelineStreamEvent('tool_started'), true);
  assert.equal(isTimelineStreamEvent('message_segment'), true);
  assert.equal(isTimelineStreamEvent('approval_required'), false);
});

test('applyStreamEventToMessages builds thinking → tool → text order', () => {
  const assistant: TurnChatMessage = {
    id: 'asst-1',
    role: 'assistant',
    content: '',
    isStreaming: true,
    blocks: [],
  };
  let messages = [assistant];
  let timeline = createEmptyTimelineState();

  ({ messages, timelineState: timeline } = applyStreamEventToMessages(
    messages,
    timeline,
    { kind: 'thinking_delta', content: 'plan' },
    { streamTargetId: 'asst-1' },
  ));
  ({ messages, timelineState: timeline } = applyStreamEventToMessages(
    messages,
    timeline,
    { kind: 'tool_started', id: 't1', name: 'read_file', input: '{}' },
    { streamTargetId: 'asst-1', currentToolId: 't1' },
  ));
  ({ messages, timelineState: timeline } = applyStreamEventToMessages(
    messages,
    timeline,
    { kind: 'tool_completed', id: 't1', success: true, output: 'ok' },
    { streamTargetId: 'asst-1', currentToolId: 't1' },
  ));
  ({ messages, timelineState: timeline } = applyStreamEventToMessages(
    messages,
    timeline,
    { kind: 'message_delta', content: 'Done.' },
    { streamTargetId: 'asst-1' },
  ));

  const blocks = messages[0].blocks ?? [];
  assert.equal(blocks.length, 3);
  assert.equal(blocks[0].kind, 'thinking');
  assert.equal(blocks[1].kind, 'tool');
  assert.equal(blocks[2].kind, 'text');
  assert.equal(messages[0].thinking, 'plan');
  assert.equal(messages[0].content, 'Done.');
  assert.equal(messages[0].tools?.length, 1);
  void timeline;
});

test('applyStreamEventToMessages clears sticky isStreaming false on non-finalize events', () => {
  const assistant: TurnChatMessage = {
    id: 'asst-1',
    role: 'assistant',
    content: '配置文件就绪。',
    isStreaming: false,
    blocks: [],
  };
  let messages = [assistant];
  let timeline = createEmptyTimelineState();

  ({ messages, timelineState: timeline } = applyStreamEventToMessages(
    messages,
    timeline,
    { kind: 'tool_started', id: 't1', name: 'write_file', input: '{}' },
    { streamTargetId: 'asst-1', currentToolId: 't1' },
  ));
  assert.equal(messages[0].isStreaming, true);

  ({ messages, timelineState: timeline } = applyStreamEventToMessages(
    messages,
    timeline,
    { kind: 'thinking_delta', content: 'next' },
    { streamTargetId: 'asst-1' },
  ));
  assert.equal(messages[0].isStreaming, true);
});

test('applyStreamEventToMessages updates preparing tool with final input', () => {
  const assistant: TurnChatMessage = {
    id: 'asst-1',
    role: 'assistant',
    content: '',
    isStreaming: true,
    blocks: [],
  };
  let messages = [assistant];
  let timeline = createEmptyTimelineState();

  ({ messages, timelineState: timeline } = applyStreamEventToMessages(
    messages,
    timeline,
    { kind: 'tool_started', id: 't1', name: 'write_file', input: null },
    { streamTargetId: 'asst-1', currentToolId: 't1' },
  ));
  const preparing = messages[0].blocks ?? [];
  assert.equal(preparing.length, 1);
  assert.equal(preparing[0].kind, 'tool');
  if (preparing[0].kind === 'tool') {
    assert.equal(preparing[0].name, 'write_file');
    assert.equal(preparing[0].input, '');
    assert.equal(preparing[0].status, 'running');
  }

  ({ messages, timelineState: timeline } = applyStreamEventToMessages(
    messages,
    timeline,
    {
      kind: 'tool_started',
      id: 't1',
      name: 'write_file',
      input: { path: 'a.rs', content: 'fn main() {}' },
    },
    { streamTargetId: 'asst-1', currentToolId: 't1' },
  ));
  const ready = messages[0].blocks ?? [];
  assert.equal(ready.length, 1);
  assert.equal(ready[0].kind, 'tool');
  if (ready[0].kind === 'tool') {
    assert.match(ready[0].input, /a\.rs/);
    assert.equal(ready[0].status, 'running');
  }
  void timeline;
});
