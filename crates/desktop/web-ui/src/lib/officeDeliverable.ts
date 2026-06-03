/** Extract workspace-relative path from write_office tool output text. */
export function parseWriteOfficeOutputPath(output: string): string | null {
  const trimmed = output.trim();
  if (!trimmed) return null;
  try {
    const j = JSON.parse(trimmed) as { metadata?: { path?: string } };
    if (typeof j.metadata?.path === 'string' && j.metadata.path.length > 0) {
      return j.metadata.path.replace(/\\/g, '/');
    }
  } catch {
    // plain text
  }
  const m = trimmed.match(/deliverables\/[^\s(（]+\.\w+/i);
  return m?.[0]?.replace(/\\/g, '/') ?? null;
}

/** Sidecar HTML preview path for an office file (matches runtime cache layout). */
export function officePreviewHtmlRelPath(officeRel: string): string {
  const base = officeRel.replace(/\\/g, '/');
  const name = base.split('/').pop() ?? base;
  return `deliverables/.office/${name}.preview.html`;
}
