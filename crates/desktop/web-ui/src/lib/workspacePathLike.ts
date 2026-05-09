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
  if (!/^[\w./\\-]+$/.test(t)) {
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
  return /^[\w./-]+$/.test(t);
}
