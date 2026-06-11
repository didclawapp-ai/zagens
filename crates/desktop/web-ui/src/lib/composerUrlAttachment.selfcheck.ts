/**
 * Self-check for Composer URL paste helpers (run: npm run test:composer-url).
 */
import assert from 'node:assert/strict';

import {
  extractPastedUrl,
  formatUrlChipLabel,
  normalizePastedUrl,
} from './composerUrlAttachment';

assert.equal(
  normalizePastedUrl('https://github.com/didclawapp-ai/zagens'),
  'https://github.com/didclawapp-ai/zagens',
);
assert.equal(
  formatUrlChipLabel('https://github.com/didclawapp-ai/zagens'),
  'github.com/didclawapp-ai/zagens',
);
assert.equal(
  formatUrlChipLabel('https://platform.deepseek.com/usage'),
  'platform.deepseek.com/usage',
);

assert.equal(
  extractPastedUrl('https://github.com/didclawapp-ai/zagens', ''),
  'https://github.com/didclawapp-ai/zagens',
);

const githubHtml =
  '<a href="https://github.com/didclawapp-ai/zagens">didclawapp-ai/zagens: Zagens — desktop agent harness</a>';
assert.equal(
  extractPastedUrl(
    'didclawapp-ai/zagens: Zagens — desktop agent harness',
    githubHtml,
  ),
  'https://github.com/didclawapp-ai/zagens',
);

assert.equal(
  extractPastedUrl('See https://example.com/docs for details', ''),
  null,
);

console.log('composerUrlAttachment self-check passed');
