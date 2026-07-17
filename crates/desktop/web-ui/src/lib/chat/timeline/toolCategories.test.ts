import { test } from 'vitest';
import assert from 'node:assert/strict';
import {
  isCollapsibleToolCategory,
  toolCategory,
} from './toolCategories';

/** High-churn registry tools that must not fall through to `other`. */
const MUST_COLLAPSE: Array<{ name: string; category: ReturnType<typeof toolCategory> }> = [
  { name: 'file_info', category: 'explore' },
  { name: 'fetch_url', category: 'explore' },
  { name: 'explore_codebase', category: 'explore' },
  { name: 'project_map', category: 'explore' },
  { name: 'describe_image', category: 'explore' },
  { name: 'git_status', category: 'explore' },
  { name: 'git_diff', category: 'explore' },
  { name: 'git_log', category: 'explore' },
  { name: 'review', category: 'explore' },
  { name: 'batch_edit', category: 'write' },
  { name: 'refactor_imports', category: 'write' },
  { name: 'restore_file', category: 'write' },
  { name: 'fim_edit', category: 'write' },
  { name: 'edit_and_check', category: 'write' },
  { name: 'run_tests', category: 'shell' },
  { name: 'exec_shell_cancel', category: 'shell' },
  { name: 'checklist_add', category: 'plan' },
  { name: 'checklist_list', category: 'plan' },
  { name: 'todo_add', category: 'plan' },
  { name: 'todo_list', category: 'plan' },
  { name: 'note', category: 'workflow' },
  { name: 'remember', category: 'workflow' },
  { name: 'recall_archive', category: 'workflow' },
  { name: 'rlm', category: 'workflow' },
  { name: 'assert_tests_pass', category: 'workflow' },
  { name: 'task_create', category: 'workflow' },
  { name: 'task_gate_run', category: 'workflow' },
  { name: 'pr_attempt_record', category: 'workflow' },
  { name: 'automation_list', category: 'workflow' },
  { name: 'wait', category: 'agent' },
  { name: 'assign_agent', category: 'agent' },
  { name: 'browser_navigate', category: 'browser' },
  { name: 'browser_snapshot', category: 'browser' },
  { name: 'browser_click', category: 'browser' },
  { name: 'browser_type', category: 'browser' },
  { name: 'browser_scroll', category: 'browser' },
  { name: 'browser_wait', category: 'browser' },
  { name: 'browser_start_preview', category: 'browser' },
];

test('registry tools used in streaming map to collapsible categories', () => {
  for (const { name, category } of MUST_COLLAPSE) {
    assert.equal(toolCategory(name), category, name);
    assert.equal(isCollapsibleToolCategory(category), true, name);
  }
});

test('interactive / meta tools stay other (expanded)', () => {
  assert.equal(toolCategory('request_user_input'), 'other');
  assert.equal(toolCategory('multi_tool_use.parallel'), 'other');
  assert.equal(isCollapsibleToolCategory('other'), false);
});
