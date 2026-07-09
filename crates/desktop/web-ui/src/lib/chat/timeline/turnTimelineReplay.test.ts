import { test } from 'vitest';
import assert from 'node:assert/strict';
import {
  buildAssistantBlocksForTurn,
  turnTimelineReplayFromThreadItems,
} from './turnTimelineReplay';
import type { TurnItemRecord } from '../../api/runtimeTypes';

const TURN_ID = 'turn_test';

test('turnTimelineReplayFromThreadItems preserves interleaved tool and text order', () => {
  const items: TurnItemRecord[] = [
    {
      schema_version: 3,
      id: 'i1',
      turn_id: TURN_ID,
      kind: 'agent_message',
      status: 'completed',
      summary: 'Planning',
      artifact_refs: [],
    },
    {
      schema_version: 3,
      id: 'i2',
      turn_id: TURN_ID,
      kind: 'tool_call',
      status: 'completed',
      summary: 'ok',
      metadata: { tool: { id: 't1', name: 'read_file', input: { path: 'a.ts' } } },
      artifact_refs: [],
    },
    {
      schema_version: 3,
      id: 'i3',
      turn_id: TURN_ID,
      kind: 'agent_message',
      status: 'completed',
      summary: 'Done reading',
      artifact_refs: [],
    },
    {
      schema_version: 3,
      id: 'i4',
      turn_id: TURN_ID,
      kind: 'file_change',
      status: 'completed',
      summary: '+1 -0',
      metadata: { tool: { id: 't2', name: 'write_file', input: { path: 'b.ts' } } },
      artifact_refs: [],
    },
  ];

  const blocks = turnTimelineReplayFromThreadItems(items, TURN_ID);
  assert.equal(blocks.length, 4);
  assert.equal(blocks[0].kind, 'text');
  assert.equal(blocks[1].kind, 'tool');
  assert.equal(blocks[1].kind === 'tool' && blocks[1].name, 'read_file');
  assert.equal(blocks[2].kind, 'text');
  assert.equal(blocks[3].kind, 'tool');
});

test('buildAssistantBlocksForTurn merges item spine with event thinking', () => {
  const items: TurnItemRecord[] = [
    {
      schema_version: 3,
      id: 'i1',
      turn_id: TURN_ID,
      kind: 'tool_call',
      status: 'completed',
      summary: 'out',
      metadata: { tool: { id: 't1', name: 'grep_files', input: {} } },
      artifact_refs: [],
    },
  ];
  const events = [
    {
      event: 'thinking.delta',
      data: JSON.stringify({ payload: { content: 'hmm' } }),
    },
    {
      event: 'tool.started',
      data: JSON.stringify({ payload: { id: 't1', name: 'grep_files', input: '{}' } }),
    },
    {
      event: 'tool.completed',
      data: JSON.stringify({ payload: { id: 't1', success: true, output: 'out' } }),
    },
  ];

  const { blocks } = buildAssistantBlocksForTurn(TURN_ID, items, events);
  assert.ok(blocks.some((b) => b.kind === 'thinking'));
  assert.ok(blocks.some((b) => b.kind === 'tool'));
});
