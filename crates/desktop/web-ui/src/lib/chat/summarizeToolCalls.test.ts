import { test } from 'vitest';
import assert from 'node:assert/strict';
import { summarizeToolCalls } from './summarizeToolCalls';

test('summarizeToolCalls', () => {

const t = (key: string, params?: Record<string, string>) => {
  const map: Record<string, string> = {
    'message.toolCallsDefault': '工具调用',
    'message.toolGroupReads': `探索 ${params?.count ?? ''} 次读取`,
    'message.toolGroupWrites': `写入 ${params?.count ?? ''} 项`,
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
      { id: '1', name: 'grep_files', input: '', status: 'done' },
      { id: '2', name: 'write_file', input: '', status: 'done' },
    ],
    t,
  ),
  'grep_files · write_file',
);
});
