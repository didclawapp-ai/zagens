import DOMPurify from 'dompurify';
import type { Config } from 'dompurify';
import {
  extractMermaidSvgStyles,
  patchMermaidSvgForWebView2,
  restoreMermaidSvgStyles,
} from './mermaidSvgPostProcess';

/** Sanitize HTML/SVG before assigning to `innerHTML` or `dangerouslySetInnerHTML`. */
export function sanitizeHtmlForDisplay(html: string): string {
  return DOMPurify.sanitize(html, {
    USE_PROFILES: { html: true, svg: true },
  });
}

const MERMAID_SVG_PURIFY: Config = {
  USE_PROFILES: { html: true, svg: true },
  ADD_TAGS: ['foreignObject', 'style'],
  ADD_ATTR: [
    'id',
    'd',
    'fill',
    'stroke',
    'stroke-width',
    'stroke-dasharray',
    'x',
    'y',
    'width',
    'height',
    'rx',
    'ry',
    'cx',
    'cy',
    'r',
    'points',
    'marker-end',
    'marker-start',
    'text-anchor',
    'font-family',
    'font-size',
    'font-weight',
    'dy',
    'dx',
    'dominant-baseline',
    'xlink:href',
    'class',
    'style',
    'xmlns',
    'data-ds-mermaid-style',
    'viewBox',
    'preserveAspectRatio',
    'markerWidth',
    'markerHeight',
    'refX',
    'refY',
    'orient',
    'markerUnits',
    'aria-roledescription',
    'role',
    'transform',
    'data-look',
    'flood-opacity',
    'flood-color',
    'stop-color',
    'stop-opacity',
    'gradientUnits',
    'offset',
  ],
  HTML_INTEGRATION_POINTS: { foreignobject: true },
};

/**
 * Sanitize Mermaid SVG for untrusted sources (legacy / fallback).
 * Trusted workspace preview and Mermaid panel skip this and use {@link scanMermaidSvgThreats}.
 * @see https://github.com/mermaid-js/mermaid/blob/develop/packages/mermaid/src/mermaidAPI.ts
 */
export function sanitizeMermaidSvg(svg: string): string {
  const { body, styles } = extractMermaidSvgStyles(svg);
  const sanitized = DOMPurify.sanitize(body, MERMAID_SVG_PURIFY);
  const restored = restoreMermaidSvgStyles(sanitized, styles);
  return patchMermaidSvgForWebView2(restored);
}

/** Sanitize highlight.js HTML (span/class only) before dangerouslySetInnerHTML. */
export function sanitizeHighlightHtml(html: string): string {
  return DOMPurify.sanitize(html, {
    ALLOWED_TAGS: ['span'],
    ALLOWED_ATTR: ['class'],
  });
}

/** Strip tags from clipboard HTML; keep plain text only. */
export function clipboardHtmlToPlainText(html: string): string {
  const clean = DOMPurify.sanitize(html, { ALLOWED_TAGS: [] });
  const tmp = document.createElement('div');
  tmp.innerHTML = clean;
  return tmp.textContent || tmp.innerText || '';
}
