import { test } from 'vitest';
import assert from 'node:assert/strict';
import { parseSubagentProgressStatus } from './subagentProgress';

test('parseSubagentProgressStatus extracts step and tool', () => {
  const p = parseSubagentProgressStatus("step 13/100: finished tool 'read_file' (ok)");
  assert.equal(p.stepsTaken, 13);
  assert.equal(p.maxSteps, 100);
  assert.equal(p.toolName, 'read_file');
  assert.equal(p.toolPhase, 'finished');
  assert.equal(p.toolOk, true);
});

test('parseSubagentProgressStatus handles running tool', () => {
  const p = parseSubagentProgressStatus("step 2/50: running tool 'grep_files'");
  assert.equal(p.stepsTaken, 2);
  assert.equal(p.toolName, 'grep_files');
  assert.equal(p.toolPhase, 'running');
  assert.equal(p.toolOk, undefined);
});
