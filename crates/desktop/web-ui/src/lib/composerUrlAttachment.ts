/** Paste-time URL detection and chip labels for Composer (Cursor-style link chips). */

const SOLE_HTTP_URL =
  /^https?:\/\/(?:[^\s<>"{}|\\^`[\]]+)$/i;

export interface UrlAttachmentFields {
  kind: 'url';
  url: string;
}

/** Normalize a pasted http(s) URL; returns null when invalid or non-http(s). */
export function normalizePastedUrl(raw: string): string | null {
  let candidate = raw.trim();
  if (!candidate) return null;
  candidate = candidate.replace(/[)\],.;]+$/g, '');
  try {
    const parsed = new URL(candidate);
    if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') return null;
    return parsed.href;
  } catch {
    return null;
  }
}

/** Chip label: host + path (no protocol), e.g. `github.com/org/repo`. */
export function formatUrlChipLabel(url: string): string {
  try {
    const u = new URL(url);
    const path = `${u.pathname}${u.search}${u.hash}`;
    if (path && path !== '/') return `${u.host}${path}`;
    return u.host;
  } catch {
    return url;
  }
}

function extractHttpHrefsFromHtml(html: string): string[] {
  const hrefs: string[] = [];
  const re = /<a\b[^>]*\shref=["']([^"']+)["']/gi;
  let match: RegExpExecArray | null;
  while ((match = re.exec(html)) !== null) {
    const href = match[1]?.trim() ?? '';
    if (/^https?:\/\//i.test(href)) hrefs.push(href);
  }
  return hrefs;
}

/**
 * When the clipboard is a lone URL (plain or HTML link), return the normalized URL.
 * GitHub-style copies often put link text in `text/plain` and the real URL in `text/html`.
 */
export function extractPastedUrl(plain: string, html: string): string | null {
  const trimmedPlain = plain.trim();
  if (trimmedPlain && SOLE_HTTP_URL.test(trimmedPlain)) {
    return normalizePastedUrl(trimmedPlain);
  }

  const trimmedHtml = html.trim();
  if (trimmedHtml) {
    const hrefs = extractHttpHrefsFromHtml(trimmedHtml);
    if (hrefs.length === 1) {
      return normalizePastedUrl(hrefs[0] ?? '');
    }
  }

  return null;
}

export function urlAttachmentFromPaste(url: string): {
  name: string;
  content: string;
  truncated: boolean;
  size: number;
  inlined: boolean;
  kind: 'url';
  url: string;
} {
  return {
    name: formatUrlChipLabel(url),
    content: '',
    truncated: false,
    size: url.length,
    inlined: false,
    kind: 'url',
    url,
  };
}
