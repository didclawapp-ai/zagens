/** Body: workspace-relative paths (CJK / Unicode filenames, percent-encoded segments). */
const WS_REL_PATH_BODY = '[\\p{L}\\p{N}._/\\\\%-]+';

/** Single- or multi-segment path under workspace root (no scheme). */
export const WORKSPACE_REL_PATH_RE = new RegExp(`^${WS_REL_PATH_BODY}$`, 'u');

/**
 * DOMPurify `ALLOWED_URI_REGEXP` for chat markdown: http(s) / mailto / ftp / tel /
 * relative workspace paths (must stay in sync with `isSafeRelativeWorkspaceHref`).
 */
export const CHAT_MARKDOWN_ALLOWED_URI = new RegExp(
  `^(?:(?:https?|ftp|mailto|tel):|(?![a-z][a-z0-9+.-]*:)(?:${WS_REL_PATH_BODY}))$`,
  'iu',
);

/**
 * Heuristic: string looks like a workspace-relative file path worth linking in chat.
 * Avoid short code tokens (e.g. `foo`, `T`) and URLs.
 */
export function isWorkspacePathlike(s: string): boolean {
  const t = s.trim();
  if (t.length === 0 || t.length > 480) {
    return false;
  }
  if (/[\s<>{}[\]`'\"]/.test(t) || t.includes('\n')) {
    return false;
  }
  if (/^(https?|mailto|ftp|vscode|file):/i.test(t)) {
    return false;
  }
  if (t.includes('..')) {
    return false;
  }
  if (t.startsWith('//')) {
    return false;
  }
  if (!WORKSPACE_REL_PATH_RE.test(t)) {
    return false;
  }

  const norm = t.replace(/\\/g, '/');
  const hasSlash = norm.includes('/');
  const namedSegment = /\.[A-Za-z0-9]{1,16}$/.test(norm);
  if (!hasSlash && !namedSegment) {
    return false;
  }

  return true;
}

/** `href` values we treat as in-workspace opens (not https:, javascript:, etc.). */
export function isSafeRelativeWorkspaceHref(href: string): boolean {
  const t = href.trim();
  if (!t || t === '#' || t.startsWith('#')) {
    return false;
  }
  if (/^[a-z][a-z0-9+.-]*:/i.test(t)) {
    return false;
  }
  if (t.includes('..') || t.startsWith('//')) {
    return false;
  }
  return WORKSPACE_REL_PATH_RE.test(t);
}
