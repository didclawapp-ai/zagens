import { test } from 'vitest';
import assert from 'node:assert/strict';

import {
  appendStreamingTextDelta,
  collapseNearDuplicateReport,
  enhanceAssistantParagraphBreaks,
  isNearDuplicateProse,
  mergeAgentMessageSegment,
} from './formatAssistantContent';
import { rebuildMessagesFromEventRecords } from './rebuildMessagesFromThread';

test('formatAssistantContent', () => {
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

  assert.equal(appendStreamingTextDelta('Let', ' me'), 'Let me', 'incremental token append');
  assert.equal(appendStreamingTextDelta('Let me ', 'Let me '), 'Let me ', 'duplicate delta skipped');
  assert.equal(
    appendStreamingTextDelta('Let me check', 'Let me check trust'),
    'Let me check trust',
    'cumulative snapshot replaces prefix',
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
});

test('mergeAgentMessageSegment keeps longer near-duplicate final report', () => {
  const v1 =
    '协同白板应用已全部构建完成。以下是完整的设计说明和运行指南。\n\n## 技术选型\n\n' +
    '后端 Go。路径 server/store.go。端口 :300。\n' +
    '功能矩阵与运行方式。'.repeat(20);
  const v2 =
    '协同白板应用已全部构建完成。以下是完整的设计说明和运行指南。\n\n## 技术选型\n\n' +
    '后端 Go。路径 server/store/store.go。端口 :3000。\n' +
    '功能矩阵与运行方式。'.repeat(22);
  assert.ok(isNearDuplicateProse(v1, v2));
  assert.equal(mergeAgentMessageSegment(v1, v2), v2);
  assert.equal(mergeAgentMessageSegment(v2, v1), v2);
});

test('collapseNearDuplicateReport keeps one half of joined duplicates', () => {
  const halfA =
    '协同白板应用已全部构建完成。以下是完整的设计说明和运行指南。\n\n## 技术选型\n\n' +
    '后端 Go。路径 server/store.go。\n' +
    '覆盖功能矩阵。'.repeat(25);
  const halfB =
    '协同白板应用已全部构建完成。以下是完整的设计说明和运行指南。\n\n## 技术选型\n\n' +
    '后端 Go。路径 server/store/store.go。\n' +
    '覆盖功能矩阵。'.repeat(28);
  const joined = `${halfA}\n\n${halfB}`;
  const collapsed = collapseNearDuplicateReport(joined);
  assert.equal(collapsed, halfB);
});

test('collapseNearDuplicateReport leaves distinct sections alone', () => {
  const a =
    '第一部分：架构说明。这里描述模块边界与依赖方向。\n' + '架构细节。'.repeat(40);
  const b =
    '第二部分：运行指南。这里描述如何启动服务与联调。\n' + '运行细节。'.repeat(40);
  const joined = `${a}\n\n${b}`;
  assert.equal(collapseNearDuplicateReport(joined), joined);
});
