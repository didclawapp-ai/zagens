import { listenCurrentWebviewEvent } from './tauriListen';

/**
 * Scoped SSE listeners for runtime stream proxy (multi-window).
 *
 * Tauri 2: `emit_to(label, …)` still delivers to every `listen()` unless the listener
 * is bound to this webview (`getCurrentWebviewWindow().listen`). See tauri#11379.
 */

export async function listenRuntimeSseEvent<T>(
  eventName: string,
  handler: (payload: T) => void,
  options?: { cancelled?: () => boolean },
): Promise<() => void> {
  return listenCurrentWebviewEvent(eventName, handler, options);
}
