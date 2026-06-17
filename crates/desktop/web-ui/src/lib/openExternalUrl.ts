const ALLOWED_SCHEMES = new Set(['http:', 'https:', 'mailto:']);

/** Returns true when `url` uses an allowed external scheme (http/https/mailto). */
export function isAllowedExternalUrl(url: string): boolean {
  const trimmed = url.trim();
  if (!trimmed) {
    return false;
  }
  try {
    return ALLOWED_SCHEMES.has(new URL(trimmed).protocol.toLowerCase());
  } catch {
    return false;
  }
}

/** Open a URL in the OS browser or mail client (Tauri invoke); falls back to window.open in dev. */
export async function openExternalUrl(url: string): Promise<void> {
  const trimmed = url.trim();
  if (!trimmed) return;
  if (!isAllowedExternalUrl(trimmed)) {
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
