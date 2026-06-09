const ALLOWED_SCHEMES = ['http:', 'https:', 'mailto:'];

/** Open a URL in the OS browser or mail client (Tauri shell); falls back to window.open in dev. */
export async function openExternalUrl(url: string): Promise<void> {
  const trimmed = url.trim();
  if (!trimmed) return;
  // Reject URLs with non-whitelisted schemes to prevent javascript:/data: URIs.
  const lower = trimmed.toLowerCase();
  if (!ALLOWED_SCHEMES.some((s) => lower.startsWith(s))) {
    console.warn('[openExternalUrl] blocked unsafe scheme:', trimmed);
    return;
  }
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('open_external_url', { url: trimmed });
  } catch {
    window.open(trimmed, '_blank', 'noopener,noreferrer');
  }
}
