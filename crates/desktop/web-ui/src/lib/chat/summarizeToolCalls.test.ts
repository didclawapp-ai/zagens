import { test } from 'vitest';
import assert from 'node:assert/strict';
import { lastToolActivityDetail, summarizeToolCalls } from './summarizeToolCalls';

test('summarizeToolCalls', () => {
  const t = (key: string, params?: Record<string, string>) => {
    const map: Record<string, string> = {
      'message.toolCallsDefault': '工具调用',
      'message.toolGroupReads': `探索 ${params?.count ?? ''} 次读取`,
      'message.toolGroupWrites': `写入 ${params?.count ?? ''} 项`,
      'message.toolGroupShell': `执行 ${params?.count ?? ''} 条命令`,
      'message.toolActivityFailed': `${params?.count ?? ''} 失败`,
      'message.toolGroupPlan': `更新计划 ${params?.count ?? ''} 次`,
      'message.toolGroupOffice': `办公文档 ${params?.count ?? ''} 次`,
      'message.toolGroupWorkflow': `工作流 ${params?.count ?? ''} 次`,
      'message.toolGroupAgent': `子代理 ${params?.count ?? ''} 次`,
      'message.toolGroupBrowser': `浏览器 ${params?.count ?? ''} 次`,
      'message.toolCallsWithName': `${params?.name ?? ''} 等 ${params?.count ?? ''} 项`,
      'message.toolCallsHeadMore': `${params?.head ?? ''} 等 ${params?.count ?? ''} 项`,
    };
    return map[key] ?? key;
  };

  assert.equal(
    summarizeToolCalls(
      [
        { id: '1', name: 'read_file', input: '', status: 'done' },
        { id: '2', name: 'read_file', input: '', status: 'done' },
      ],
      t,
    ),
    '探索 2 次读取',
  );

  assert.equal(
    summarizeToolCalls(
      [
        { id: '1', name: 'grep_files', input: '', status: 'done' },
        { id: '2', name: 'read_file', input: '', status: 'running' },
        { id: '3', name: 'grep_files', input: '', status: 'done' },
      ],
      t,
    ),
    '探索 3 次读取',
    'grep_files + read_file should aggregate as explore reads',
  );

  assert.equal(
    summarizeToolCalls([{ id: '1', name: 'write_file', input: '', status: 'running' }], t),
    '写入 1 项',
  );

  assert.equal(
    summarizeToolCalls(
      [
        {
          id: '1',
          name: 'update_plan',
          input: JSON.stringify({ steps: ['review', 'ship'] }),
          status: 'done',
        },
        {
          id: '2',
          name: 'checklist_write',
          input: JSON.stringify({ items: ['verify build'] }),
          status: 'done',
        },
      ],
      t,
    ),
    '更新计划 2 次',
    'plan tools collapse under toolGroupPlan',
  );

  assert.equal(
    summarizeToolCalls(
      [
        {
          id: '1',
          name: 'load_skill',
          input: JSON.stringify({ name: 'audit-repo' }),
          status: 'done',
        },
        {
          id: '2',
          name: 'scratchpad_init',
          input: JSON.stringify({ template: 'workspace_audit' }),
          status: 'done',
        },
      ],
      t,
    ),
    '工作流 2 次 · workspace_audit',
    'load_skill + scratchpad collapse under toolGroupWorkflow',
  );

  assert.equal(
    summarizeToolCalls(
      [
        {
          id: '1',
          name: 'agent_spawn',
          input: JSON.stringify({ type: 'explorer', prompt: 'review runtime' }),
          status: 'done',
        },
        {
          id: '2',
          name: 'agent_wait',
          input: JSON.stringify({ agent_id: 'agent_304e162e' }),
          status: 'done',
        },
      ],
      t,
    ),
    '子代理 2 次 · agent_304e162e',
    'agent_spawn family collapses under toolGroupAgent',
  );

  assert.equal(
    summarizeToolCalls(
      [
        {
          id: '1',
          name: 'browser_navigate',
          input: JSON.stringify({ url: 'http://127.0.0.1:8080/' }),
          status: 'done',
        },
        {
          id: '2',
          name: 'browser_click',
          input: JSON.stringify({ ref: 'button:add-task:0' }),
          status: 'done',
        },
      ],
      t,
    ),
    '浏览器 2 次 · button:add-task:0',
    'browser_* tools collapse under toolGroupBrowser',
  );

  assert.equal(
    summarizeToolCalls(
      [
        { id: '1', name: 'grep_files', input: '', status: 'done' },
        { id: '2', name: 'write_file', input: '', status: 'done' },
      ],
      t,
    ),
    '写入 1 项 · 探索 1 次读取',
    'mixed categories summarize by category counts',
  );

  assert.equal(
    summarizeToolCalls(
      [
        {
          id: '1',
          name: 'write_file',
          input: JSON.stringify({ path: 'server/store/jsonstore.go' }),
          status: 'done',
        },
        {
          id: '2',
          name: 'edit_file',
          input: JSON.stringify({ path: 'whiteboard/server/main.go' }),
          status: 'done',
        },
        {
          id: '3',
          name: 'exec_shell',
          input: JSON.stringify({ command: 'go build ./...' }),
          status: 'done',
        },
      ],
      t,
    ),
    '写入 2 项 · 执行 1 条命令 · go build ./...',
    'appends last shell command after counts',
  );

  assert.equal(
    lastToolActivityDetail([
      {
        id: '1',
        name: 'exec_shell',
        input: JSON.stringify({ command: 'npm install' }),
        status: 'done',
      },
      {
        id: '2',
        name: 'write_file',
        input: JSON.stringify({ path: 'E:\\\\61\\\\client\\\\app.js' }),
        status: 'done',
      },
    ]),
    'app.js',
    'uses chronological last tool (write basename)',
  );

  assert.equal(
    lastToolActivityDetail([
      {
        id: '1',
        name: 'write_file',
        input: JSON.stringify({ path: 'server/main.go' }),
        status: 'done',
      },
      {
        id: '2',
        name: 'exec_shell',
        input: JSON.stringify({ command: 'go build ./...' }),
        status: 'done',
      },
    ]),
    'go build ./...',
    'uses chronological last tool (shell command)',
  );

  assert.equal(
    summarizeToolCalls(
      [
        { id: '1', name: 'exec_shell', input: '{}', status: 'done' },
        { id: '2', name: 'exec_shell', input: '{}', status: 'error' },
        { id: '3', name: 'exec_shell', input: '{}', status: 'done' },
      ],
      t,
    ),
    '执行 3 条命令 · 1 失败',
    'surfaces failed count in shell activity summary',
  );

  assert.equal(
    summarizeToolCalls(
      [
        { id: '1', name: 'exec_shell', input: '{}', status: 'done' },
        {
          id: '2',
          name: 'exec_shell',
          input: JSON.stringify({ command: 'npm install' }),
          status: 'error',
        },
      ],
      t,
      { captions: ['安装依赖并验证编译。'] },
    ),
    '安装依赖并验证编译。 · 执行 2 条命令 · 1 失败 · npm install',
    'caption leads the activity label (thr_ea9c)',
  );
});
