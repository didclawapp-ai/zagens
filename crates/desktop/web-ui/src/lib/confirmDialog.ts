/** Native confirm in Tauri (ACL-safe) or browser fallback. */
export async function confirmDialog(
  message: string,
  title = 'DS Pick',
): Promise<boolean> {
  try {
    const { ask } = await import('@tauri-apps/plugin-dialog');
    return await ask(message, { title, kind: 'warning' });
  } catch {
    return window.confirm(message);
  }
}
