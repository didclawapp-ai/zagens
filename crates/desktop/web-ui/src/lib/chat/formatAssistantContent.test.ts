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
  assert.equal(
    appendStreamingTextDelta('Let me check the trust settings ', 'Let me check the trust settings '),
    'Let me check the trust settings ',
    'long duplicate replay chunk skipped',
  );
  assert.equal(
    appendStreamingTextDelta('Let me check the trust', 'Let me check the trust settings now'),
    'Let me check the trust settings now',
    'long replayed overlap extends the accumulated text',
  );
  // Regression (thr_65ce8faa): short legit deltas that repeat the current suffix
  // must never be dropped — "100"+"0" and "smart"+"art" are real token splits.
  assert.equal(appendStreamingTextDelta('尺寸 100', '0'), '尺寸 1000', 'suffix-colliding digit kept');
  assert.equal(
    appendStreamingTextDelta('result.smart', 'art'),
    'result.smartart',
    'suffix-colliding token kept',
  );
  assert.equal(appendStreamingTextDelta('好。', '好。'), '好。好。', 'short repeat appended, not deduped');
  // Volume-amplifying regression: a fresh batch that happens to equal an earlier
  // mid-string substring must still append. Mid-string `includes` false positives
  // only show up after the bubble has grown (multi-tool / multi-turn prose).
  const longAccum =
    '结构清楚。现在并行定位问题源码——先读动画模块。继续定位：函数名不同。放宽搜索。';
  const freshBatch = '继续定位：函数名不同'; // appears earlier, but this is new prose later
  assert.equal(
    appendStreamingTextDelta(longAccum, freshBatch),
    longAccum + freshBatch,
    'mid-string collision must not drop a fresh batch',
  );
  const trailingReplay = '继续定位：函数名不同。放宽搜索。';
  assert.ok(trailingReplay.length >= 16);
  assert.equal(
    appendStreamingTextDelta(longAccum, trailingReplay),
    longAccum,
    'true trailing replay of a long chunk is still skipped',
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

test('mergeAgentMessageSegment replaces lossy streamed copy instead of appending', () => {
  // Regression (thr_65ce8faa item_477e1865): delta dedup swallowed one "0",
  // so the streamed bubble held a lossy copy; the completed segment must
  // replace it, not append the whole message a second time.
  const full =
    'G/H 前半完成。G4 显示编辑后 sheet 尺寸 1000×702 异常——对比原始文件验证行列操作是否造成膨胀。';
  const lossy = full.replace('1000×702', '100×702');
  assert.equal(mergeAgentMessageSegment(lossy, full), full, 'lossy copy replaced by full text');
  // Distinct short segments still concatenate.
  assert.equal(
    mergeAgentMessageSegment('第一段结论。', '接下来是完全不同的第二段补充说明内容。'),
    '第一段结论。\n\n接下来是完全不同的第二段补充说明内容。',
    'distinct segments still merge with a gap',
  );
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
