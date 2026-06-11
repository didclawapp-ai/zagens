import assert from 'node:assert/strict';

import {
  pickBestSessionMessages,
  sessionMessageRichness,
  snapshotHasAssistantMeta,
} from './sessionMessagePick';
import type { CachedUiMessage } from './sessionUiCache';

const fragmentedSession: CachedUiMessage[] = [
  { id: 'u1', role: 'user', content: '分析仓库结构' },
  { id: 'a1', role: 'assistant', content: '先梳理仓库骨架。' },
  { id: 'a2', role: 'assistant', content: '继续查看核心 crate 的入口。' },
  { id: 'a3', role: 'assistant', content: '查看两个关键入口文件。' },
  { id: 'a4', role: 'assistant', content: '再快速确认两个关键 crate。' },
  {
    id: 'a5',
    role: 'assistant',
    content: '以下是仓库骨架的完整梳理。\n\n## 仓库结构总览',
  },
];

const richThread: CachedUiMessage[] = [
  { id: 'u1', role: 'user', content: '分析仓库结构' },
  {
    id: 'a1',
    role: 'assistant',
    content: '先梳理仓库骨架。\n\n继续查看核心 crate 的入口。\n\n以下是仓库骨架的完整梳理。',
    thinking: '先梳理仓库骨架。\n继续查看核心 crate。',
    tools: [
      {
        id: 't1',
        name: 'read_file',
        input: '{"path":"Cargo.toml"}',
        output: 'ok',
        status: 'done',
      },
      {
        id: 't2',
        name: 'grep',
        input: '{"pattern":"main"}',
        output: 'matches',
        status: 'done',
      },
    ],
  },
];

assert.ok(
  sessionMessageRichness(richThread) > sessionMessageRichness(fragmentedSession),
  'consolidated thread replay with tools/thinking must beat fragmented session JSON',
);

const picked = pickBestSessionMessages([
  { source: 'session', messages: fragmentedSession },
  { source: 'thread', messages: richThread },
]);
assert.equal(picked, richThread, 'thread replay wins over session fallback');
assert.ok(snapshotHasAssistantMeta(richThread));
assert.ok(!snapshotHasAssistantMeta(fragmentedSession));

console.log('sessionMessagePick self-check passed');
