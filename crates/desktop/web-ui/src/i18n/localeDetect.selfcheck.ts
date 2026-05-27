/**
 * Self-check for system locale matching (run: npm run test:i18n-locale).
 */
import assert from 'node:assert/strict';

import { detectLocaleFromSystem, matchLocaleFromTag } from './utils';

assert.equal(matchLocaleFromTag('zh-CN'), 'zh-Hans');
assert.equal(matchLocaleFromTag('zh-Hans'), 'zh-Hans');
assert.equal(matchLocaleFromTag('zh-TW'), null);
assert.equal(matchLocaleFromTag('zh-HK'), null);
assert.equal(matchLocaleFromTag('ja-JP'), 'ja');
assert.equal(matchLocaleFromTag('pt-BR'), 'pt-BR');
assert.equal(matchLocaleFromTag('pt-PT'), 'pt-BR');
assert.equal(matchLocaleFromTag('en-US'), 'en');
assert.equal(matchLocaleFromTag('de-DE'), null);
assert.equal(matchLocaleFromTag('fr-FR'), null);

assert.equal(detectLocaleFromSystem(['de-DE', 'en-US']), 'en');
assert.equal(detectLocaleFromSystem(['zh-TW', 'en-US']), 'en');
assert.equal(detectLocaleFromSystem(['zh-CN']), 'zh-Hans');
assert.equal(detectLocaleFromSystem(['ja', 'en']), 'ja');
assert.equal(detectLocaleFromSystem(['ko-KR']), 'en');
assert.equal(detectLocaleFromSystem([]), 'en');

console.log('localeDetect self-check passed');
