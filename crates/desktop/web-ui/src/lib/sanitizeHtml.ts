import DOMPurify from 'dompurify';

/** Sanitize HTML/SVG before assigning to `innerHTML` or `dangerouslySetInnerHTML`. */
export function sanitizeHtmlForDisplay(html: string): string {
  return DOMPurify.sanitize(html, {
    USE_PROFILES: { html: true, svg: true },
  });
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
