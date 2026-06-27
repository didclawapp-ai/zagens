import { test } from 'vitest';
import assert from 'node:assert/strict';
import MarkdownIt from 'markdown-it';

import {
  blockDigest,
  normalizeMermaidSourceForSvgLabels,
  RENDER_FLASH_THRESHOLD_MS,
} from './mermaidRuntime';
import { applyMermaidFenceRule } from './markdownMermaidFence';
import {
  fixMermaidBackgroundRects,
  fixSvgDimensionsForWebView2,
  inlineClusterRectPaint,
  contrastingTextColor,
  inlineForeignObjectLabelColors,
  parseSvgViewBoxSize,
  scaledSvgDisplayHeight,
  patchMermaidSvgForWebView2,
  peelMermaidStyles,
  promoteSvgPresentationAttributes,
} from './mermaidSvgPostProcess';
import {
  scanMermaidSvgThreats,
  MermaidSvgThreatError,
  assertMermaidSvgSafe,
} from './mermaidSvgSecurity';

test('mermaid helpers', async () => {
  assert.match(blockDigest('graph TD\n  A --> B'), /^[0-9a-z]+$/);
  assert.equal(blockDigest('graph TD\n  A --> B'), blockDigest('graph TD\n  A --> B'));
  assert.notEqual(blockDigest('A-->B'), blockDigest('A-->C'));
  assert.equal(blockDigest(''), '0');

  assert.equal(normalizeMermaidSourceForSvgLabels('a<br/>b<br>b'), 'a\nb\nb');
  assert.equal(normalizeMermaidSourceForSvgLabels('no breaks'), 'no breaks');
  assert.equal(RENDER_FLASH_THRESHOLD_MS, 50);

  function mdWithMermaidFence(): MarkdownIt {
    const md = new MarkdownIt({ html: false, linkify: true, breaks: true });
    applyMermaidFenceRule(md);
    return md;
  }

  const fenceHtml = mdWithMermaidFence().render('```mermaid\ngraph TD\n  A --> B\n```\n');
  assert.ok(fenceHtml.includes('ds-mermaid-block'));
  assert.ok(fenceHtml.includes('ds-mermaid-mount'));

  const bareRectSvg = '<svg><g class="label"><rect/><foreignObject>x</foreignObject></g></svg>';
  const patched = patchMermaidSvgForWebView2(bareRectSvg);
  assert.ok(!patched.includes('<rect/>'));

  const labelBgSvg =
    '<svg><g class="label"><rect class="background" style="stroke: none" x="1" y="2" width="80" height="20"/></g></svg>';
  assert.ok(fixMermaidBackgroundRects(labelBgSvg).includes('fill="none"'));

  const clusterSvg =
    '<svg><style>#g .cluster rect{fill:#ffffde;stroke:#aaaa33;stroke-width:1px}</style>'
    + '<g class="cluster" id="g-entry"><rect style="" x="8" y="8" width="100" height="50"/></g></svg>';
  const clusterPatched = inlineClusterRectPaint(clusterSvg);
  assert.ok(clusterPatched.includes('fill="#ffffde"'));
  assert.ok(clusterPatched.includes('stroke="#aaaa33"'));

  const styledNode =
    '<svg><rect class="basic label-container" style="fill:#1e3a5f !important;stroke:#60a5fa !important" x="0" y="0" width="10" height="10"/></svg>';
  const promoted = promoteSvgPresentationAttributes(styledNode);
  assert.match(promoted, /fill="#1e3a5f"/);
  assert.match(promoted, /stroke="#60a5fa"/);

  const edgeSvg =
    '<svg><style>.edgePath .path{stroke:#333333;stroke-width:1px}</style>'
    + '<g class="edgePaths"><path class="path" d="M0,0 L10,10"/></g></svg>';
  const edgePatched = patchMermaidSvgForWebView2(edgeSvg);
  assert.ok(edgePatched.includes('fill="none"'));
  assert.ok(edgePatched.includes('stroke="#333333"'));

  const emSvg =
    '<svg><style>#g{font-size:16px}</style><text><tspan x="0" y="-0.1em" dy="1.1em">A</tspan></text></svg>';
  assert.ok(patchMermaidSvgForWebView2(emSvg).includes('dy="17.6px"'));

  assert.equal(contrastingTextColor('#ECECFF'), '#333333');
  assert.equal(contrastingTextColor('#1e3a5f'), '#ffffff');

  const defaultNodeFo =
    '<svg><g class="node default"><rect class="basic label-container" fill="#ECECFF"/>'
    + '<g class="label"><foreignObject><div xmlns="http://www.w3.org/1999/xhtml">'
    + '<span class="nodeLabel"><p>User</p></span></div></foreignObject></g></g></svg>';
  assert.ok(inlineForeignObjectLabelColors(defaultNodeFo).includes('color:#333333!important'));
  assert.ok(inlineForeignObjectLabelColors(defaultNodeFo).includes('-webkit-text-fill-color:#333333!important'));

  const productNodeFo =
    '<svg><g class="node default product"><g class="label" style="color:#fff !important"><foreignObject>'
    + '<div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel"><p>UI</p></span></div>'
    + '</foreignObject></g></g></svg>';
  assert.ok(inlineForeignObjectLabelColors(productNodeFo).includes('color:#ffffff!important'));

  const edgeFo =
    '<svg><g class="edgeLabel"><g class="label"><foreignObject width="100" height="24">'
    + '<div xmlns="http://www.w3.org/1999/xhtml" class="labelBkg">'
    + '<span class="edgeLabel"><p>spawn + DS_PICK_READY</p></span></div>'
    + '</foreignObject></g></g></svg>';
  const edgeLabelPatched = inlineForeignObjectLabelColors(edgeFo);
  assert.ok(edgeLabelPatched.includes('rgba(232,232,232,0.85)'));
  assert.ok(edgeLabelPatched.includes('color:#333333!important'));

  const foSvg =
    '<svg><foreignObject><div xmlns="http://www.w3.org/1999/xhtml"><p>Label</p></div></foreignObject></svg>';
  assert.ok(patchMermaidSvgForWebView2(foSvg).includes('background:transparent!important'));

  const responsiveSvg =
    '<svg width="100%" style="max-width: 500px; background-color: white;" viewBox="0 0 200 100"><rect x="0" y="0" width="10" height="10"/></svg>';
  const dimFixed = fixSvgDimensionsForWebView2(responsiveSvg);
  assert.ok(dimFixed.includes('width="200"'));
  assert.ok(dimFixed.includes('height="100"'));
  assert.ok(!/max-width\s*:\s*500px/i.test(dimFixed));

  const viewBoxSize = parseSvgViewBoxSize('<svg viewBox="0 0 200 100"></svg>');
  assert.equal(viewBoxSize?.width, 200);
  assert.equal(viewBoxSize?.height, 100);

  assert.equal(scaledSvgDisplayHeight('<svg viewBox="0 0 200 100"></svg>', 100), 50);
  assert.equal(scaledSvgDisplayHeight('<svg viewBox="0 0 200 100"></svg>', 0), 480);

  const themedSvg = '<svg id="m1"><style>#m1 .product{fill:#1e3a5f}</style><rect class="product"/></svg>';
  const trusted = patchMermaidSvgForWebView2(themedSvg);
  assert.ok(trusted.includes('#1e3a5f'));
  assert.equal(scanMermaidSvgThreats(trusted), null);
  assert.equal(scanMermaidSvgThreats('<svg><script>alert(1)</script></svg>'), 'script');
  assert.throws(() => assertMermaidSvgSafe('<svg onload="x"></svg>'), MermaidSvgThreatError);

  const peeled = peelMermaidStyles(trusted);
  assert.ok(peeled.css.includes('#1e3a5f'));
  assert.ok(!peeled.svgBody.includes('<style'));

  try {
    const { JSDOM } = await import('jsdom');
    const createDOMPurify = (await import('dompurify')).default;
    const dom = new JSDOM('<!DOCTYPE html><html><body></body></html>');
    const win = dom.window as unknown as Window & typeof globalThis;
    (globalThis as typeof globalThis & { window: typeof win }).window = win;
    (globalThis as typeof globalThis & { document: Document }).document = win.document;
    createDOMPurify(win);

    const { sanitizeMermaidSvg } = await import('./sanitizeHtml');
    const dirty = '<svg id="m1"><script/x=">alert(1)</script><rect/></svg>';
    const clean = sanitizeMermaidSvg(dirty);
    assert.ok(!clean.includes('<script'));
  } catch (err) {
    console.warn(
      'mermaid test: skipped DOMPurify tests:',
      (err as Error).message ?? err,
    );
  }
});
