/**
 * Scoped SSE listeners for runtime stream proxy (multi-window).
 *
 * Tauri 2: `emit_to(label, …)` still delivers to every `listen()` unless the listener
 * is bound to this webview (`getCurrentWebviewWindow().listen`). See tauri#11379.
 */

export async function listenRuntimeSseEvent<T>(
  eventName: string,
  handler: (payload: T) => void,
): Promise<() => void> {
  const { getCurrentWebviewWindow } = await import('@tauri-apps/api/webviewWindow');
  const unlisten = await getCurrentWebviewWindow().listen<T>(eventName, (ev) => {
    handler(ev.payload);
  });
  return unlisten;
}
