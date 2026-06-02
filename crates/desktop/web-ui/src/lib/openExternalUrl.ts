/** Open a URL in the OS browser or mail client (Tauri shell); falls back to window.open in dev. */
export async function openExternalUrl(url: string): Promise<void> {
  const trimmed = url.trim();
  if (!trimmed) return;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('open_external_url', { url: trimmed });
  } catch {
    window.open(trimmed, '_blank', 'noopener,noreferrer');
  }
}
