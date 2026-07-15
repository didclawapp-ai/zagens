import { test } from 'vitest';
import assert from 'node:assert/strict';
import {
  collectHtmlAssetRefs,
  resolveHtmlAssetToWorkspaceRel,
  rewriteHtmlAssetUrls,
  textToDataUrl,
} from './htmlPreviewAssets';

test('resolveHtmlAssetToWorkspaceRel resolves sibling relative paths', () => {
  assert.equal(
    resolveHtmlAssetToWorkspaceRel('docs/demo/index.html', 'styles.css'),
    'docs/demo/styles.css',
  );
  assert.equal(
    resolveHtmlAssetToWorkspaceRel('docs/demo/index.html', './img/logo.png'),
    'docs/demo/img/logo.png',
  );
});

test('resolveHtmlAssetToWorkspaceRel resolves root-relative from workspace root', () => {
  assert.equal(
    resolveHtmlAssetToWorkspaceRel('docs/demo/index.html', '/assets/app.css'),
    'assets/app.css',
  );
});

test('resolveHtmlAssetToWorkspaceRel rejects path escape above workspace', () => {
  assert.equal(resolveHtmlAssetToWorkspaceRel('docs/a.html', '../../etc/passwd'), null);
  assert.equal(resolveHtmlAssetToWorkspaceRel('a.html', '../secret'), null);
});

test('resolveHtmlAssetToWorkspaceRel leaves external schemes alone', () => {
  assert.equal(
    resolveHtmlAssetToWorkspaceRel('index.html', 'https://cdn.example/x.css'),
    null,
  );
  assert.equal(resolveHtmlAssetToWorkspaceRel('index.html', 'javascript:alert(1)'), null);
  assert.equal(resolveHtmlAssetToWorkspaceRel('index.html', 'data:text/css,body{}'), null);
});

test('collectHtmlAssetRefs finds link/script/img and ignores anchors', () => {
  const html = `
    <html><head>
      <link rel="stylesheet" href="a.css">
      <script src="./b.js"></script>
    </head><body>
      <img src="pic.png" alt="x">
      <a href="other.html">nav</a>
    </body></html>
  `;
  const refs = collectHtmlAssetRefs(html);
  const urls = refs.map((r) => r.url).sort();
  assert.deepEqual(urls, ['./b.js', 'a.css', 'pic.png']);
});

test('rewriteHtmlAssetUrls substitutes only mapped asset urls', () => {
  const html = `<link href="a.css"><img src="b.png"><a href="c.html">`;
  const map = new Map<string, string>([
    ['a.css', textToDataUrl('text/css;charset=utf-8', 'body{color:red}')],
  ]);
  const out = rewriteHtmlAssetUrls(html, map);
  assert.match(out, /link href="data:text\/css/);
  assert.match(out, /img src="b\.png"/);
  assert.match(out, /a href="c\.html"/);
});
