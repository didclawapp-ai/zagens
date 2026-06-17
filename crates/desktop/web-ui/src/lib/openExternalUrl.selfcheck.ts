/**
 * Self-check for external URL scheme guard (run: npm run test:open-external-url).
 */
import assert from 'node:assert/strict';

import { isAllowedExternalUrl } from './openExternalUrl';

assert.equal(isAllowedExternalUrl('https://zagens.com/docs'), true);
assert.equal(isAllowedExternalUrl('http://127.0.0.1:1420'), true);
assert.equal(isAllowedExternalUrl('mailto:support@example.com'), true);
assert.equal(isAllowedExternalUrl('javascript:alert(1)'), false);
assert.equal(isAllowedExternalUrl('data:text/html,hi'), false);
assert.equal(isAllowedExternalUrl('ftp://example.com'), false);
assert.equal(isAllowedExternalUrl(''), false);

console.log('openExternalUrl.selfcheck: ok');
