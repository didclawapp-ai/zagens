/** Safe Tauri event listen/unlisten helpers (multi-window + Strict Mode). */

export function safeUnlisten(unlisten?: (() => void) | null): void {
  if (!unlisten) return;
  try {
    unlisten();
  } catch {
    /* listener may already be removed or webview torn down */
  }
}

/**
 * Register a listener on the current webview. Returns an unlisten fn that is safe
 * to call multiple times. If `cancelled` is already true when listen resolves,
 * unlisten immediately instead of handing the fn to the caller.
 */
export async function listenCurrentWebviewEvent<T>(
  eventName: string,
  handler: (payload: T) => void,
  options?: { cancelled?: () => boolean },
): Promise<() => void> {
  const { getCurrentWebviewWindow } = await import('@tauri-apps/api/webviewWindow');
  const unlisten = await getCurrentWebviewWindow().listen<T>(eventName, (ev) => {
    handler(ev.payload);
  });
  if (options?.cancelled?.()) {
    safeUnlisten(unlisten);
    return () => {};
  }
  return () => safeUnlisten(unlisten);
}

export function createListenerRegistry(): {
  add: (unlisten: () => void) => void;
  finish: () => void;
  isSettled: () => boolean;
} {
  let settled = false;
  const unsubs: Array<() => void> = [];

  const add = (unlisten: () => void) => {
    if (settled) {
      safeUnlisten(unlisten);
      return;
    }
    unsubs.push(() => safeUnlisten(unlisten));
  };

  const finish = () => {
    if (settled) return;
    settled = true;
    for (const u of unsubs) {
      u();
    }
    unsubs.length = 0;
  };

  return { add, finish, isSettled: () => settled };
}

/**
 * Subscribe to a webview event from a React effect. Handles Strict Mode cleanup
 * races where `listen()` resolves after the effect has already unmounted.
 */
export function subscribeCurrentWebviewEvent<T>(
  eventName: string,
  handler: (payload: T) => void,
): () => void {
  let cancelled = false;
  let unlisten: (() => void) | undefined;

  void listenCurrentWebviewEvent(eventName, handler, { cancelled: () => cancelled }).then((fn) => {
    if (cancelled) {
      safeUnlisten(fn);
      return;
    }
    if (unlisten) {
      safeUnlisten(unlisten);
    }
    unlisten = fn;
  });

  return () => {
    cancelled = true;
    safeUnlisten(unlisten);
    unlisten = undefined;
  };
}
