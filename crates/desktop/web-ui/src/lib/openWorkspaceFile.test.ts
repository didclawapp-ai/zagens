import { test } from 'vitest';
import assert from 'node:assert/strict';
import { normalizeWorkspaceRelPath } from './openWorkspaceFile';

test('normalizeWorkspaceRelPath strips leading slashes and backslashes', () => {
  assert.equal(normalizeWorkspaceRelPath('\\doc\\a.md'), 'doc/a.md');
  assert.equal(normalizeWorkspaceRelPath('/doc/a.md'), 'doc/a.md');
  assert.equal(normalizeWorkspaceRelPath('./doc/a.md'), 'doc/a.md');
});

test('normalizeWorkspaceRelPath decodes markdown-it percent-encoded CJK hrefs', () => {
  // Same encoding markdown-it emits for:
  // [doc/OL-QMS全库代码审核报告-v3.0.1.md](doc/OL-QMS全库代码审核报告-v3.0.1.md)
  const encoded =
    'doc/OL-QMS%E5%85%A8%E5%BA%93%E4%BB%A3%E7%A0%81%E5%AE%A1%E6%A0%B8%E6%8A%A5%E5%91%8A-v3.0.1.md';
  assert.equal(
    normalizeWorkspaceRelPath(encoded),
    'doc/OL-QMS全库代码审核报告-v3.0.1.md',
  );
});

test('normalizeWorkspaceRelPath leaves already-decoded paths unchanged', () => {
  assert.equal(
    normalizeWorkspaceRelPath('doc/OL-QMS全库代码审核报告-v3.0.1.md'),
    'doc/OL-QMS全库代码审核报告-v3.0.1.md',
  );
});

test('normalizeWorkspaceRelPath tolerates malformed percent sequences', () => {
  assert.equal(normalizeWorkspaceRelPath('doc/bad%ZZname.md'), 'doc/bad%ZZname.md');
});
