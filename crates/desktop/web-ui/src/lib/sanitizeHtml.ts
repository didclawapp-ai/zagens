import DOMPurify from 'dompurify';
import type { Config } from 'dompurify';

/** Sanitize HTML/SVG before assigning to `innerHTML` or `dangerouslySetInnerHTML`. */
export function sanitizeHtmlForDisplay(html: string): string {
  return DOMPurify.sanitize(html, {
    USE_PROFILES: { html: true, svg: true },
  });
}

/**
 * Sanitize Mermaid SVG output while preserving label HTML inside `foreignObject`.
 * DOMPurify 3.1.7+ strips that content by default, which makes mindmap/flowchart text invisible.
 * @see https://github.com/mermaid-js/mermaid/blob/develop/packages/mermaid/src/mermaidAPI.ts
 */
export function sanitizeMermaidSvg(svg: string): string {
  const config: Config = {
    USE_PROFILES: { html: true, svg: true },
    ADD_TAGS: ['foreignObject', 'style'],
    ADD_ATTR: ['dominant-baseline', 'class', 'style', 'xmlns'],
    HTML_INTEGRATION_POINTS: { foreignobject: true },
  };
  return DOMPurify.sanitize(svg, config);
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
