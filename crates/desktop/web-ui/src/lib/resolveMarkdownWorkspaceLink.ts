/**
 * Resolve a Markdown link href to a workspace-root-relative path (POSIX slashes).
 * Returns null for fragment-only links, external schemes, or paths that escape above workspace root.
 */
export function resolveMarkdownLinkToWorkspaceRel(
  baseWorkspaceRel: string | undefined,
  href: string,
): string | null {
  const raw = href.trim();
  if (!raw || raw === '#') {
    return null;
  }
  if (raw.startsWith('#')) {
    return null;
  }
  if (/^[a-z][a-z0-9+.-]*:/i.test(raw)) {
    return null;
  }

  const normHref = raw.replace(/\\/g, '/');
  const fromWorkspaceRoot = normHref.startsWith('/');
  const pathPart = fromWorkspaceRoot ? normHref.slice(1) : normHref;
  const base = (baseWorkspaceRel ?? '').trim().replace(/\\/g, '/');
  const baseDir = fromWorkspaceRoot ? '' : base.includes('/') ? base.replace(/\/[^/]+$/, '') : '';

  const hrefParts = pathPart.split('/').filter((p) => p !== '' && p !== '.');
  const baseParts = baseDir ? baseDir.split('/').filter(Boolean) : [];

  const stack = fromWorkspaceRoot ? [] : [...baseParts];
  for (const p of hrefParts) {
    if (p === '..') {
      if (stack.length === 0) {
        return null;
      }
      stack.pop();
    } else {
      stack.push(p);
    }
  }

  if (stack.length === 0) {
    return null;
  }
  return stack.join('/');
}
