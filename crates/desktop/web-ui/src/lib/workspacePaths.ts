/**
 * Join Composer workspace root with path segments using native-style separators for display/Tauri opens.
 */
export function joinWorkspaceSegments(root: string, ...segments: string[]): string {
  const r = root.trim().replace(/[/\\]+$/, '');
  if (!r) {
    return segments
      .map((s) => s.replace(/^[/\\]+|[/\\]+$/g, ''))
      .filter(Boolean)
      .join('/');
  }
  const sep = r.includes('\\') ? '\\' : '/';
  const cleaned = segments.map((s) => s.replace(/^[/\\]+|[/\\]+$/g, '')).filter(Boolean);
  return [r, ...cleaned].join(sep);
}
