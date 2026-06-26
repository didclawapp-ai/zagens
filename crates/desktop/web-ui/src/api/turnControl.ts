/**
 * D9 — Two-layer turn stop contract (desktop shell).
 *
 * Layer 1 (`disconnectThreadEventStream`): abort local SSE / poll consumption +
 * `runtime_cancel_sse` (scoped to `threadId` when provided — P0.1 multi-session).
 * Layer 2 (`interruptThreadTurn` HTTP): `Op::Interrupt` on the runtime — stops LLM/tools.
 *
 * UI **Stop** calls `stopThreadTurn`: layer 1 first (instant UI cutoff), then layer 2.
 * Layer 1 alone leaves the turn running on the backend.
 *
 * SSOT: docs/tech/API_DESIGN.md §2.1.1 · RUNTIME_ARCHITECTURE.md §8
 */

import type { TurnRecord } from './client';
import { interruptThreadTurn as interruptThreadTurnHttp } from './client';

export type { TurnRecord };

/**
 * Layer 1 only — disconnect WebView ↔ sidecar event pipe; does not stop the runtime turn.
 *
 * `threadId` (P0.1): when provided, only the SSE consumer for that thread is
 * cancelled in the Rust proxy; other concurrently-streaming threads in the
 * same window are left alone. Omitting it cancels every in-flight SSE for the
 * window (legacy global-stop behaviour).
 */
export function disconnectThreadEventStream(
  streamControl?: AbortController | AbortSignal,
  threadId?: string,
): void {
  if (streamControl instanceof AbortController) {
    streamControl.abort();
  }
  if (typeof window === 'undefined' || !('__TAURI_INTERNALS__' in window)) {
    return;
  }
  const tid = threadId?.trim() || undefined;
  void import('@tauri-apps/api/core')
    .then(({ invoke }) => invoke('runtime_cancel_sse', { threadId: tid }))
    .catch(() => {
      /* sidecar may already be done */
    });
}

/** Layer 2 — runtime `POST …/interrupt` (existing HTTP helper). */
export { interruptThreadTurnHttp as interruptThreadTurn };

/** User Stop / Escape: interrupt runtime turn and tear down local stream (D9 unified API). */
export async function stopThreadTurn(params: {
  threadId: string;
  turnId: string;
  streamControl?: AbortController | AbortSignal;
}): Promise<TurnRecord | undefined> {
  const threadId = params.threadId.trim();
  const turnId = params.turnId.trim();
  // Tear down the local SSE pipe first so the UI stops receiving deltas immediately.
  disconnectThreadEventStream(params.streamControl, threadId || undefined);

  let interruptResult: TurnRecord | undefined;
  if (threadId && turnId) {
    try {
      interruptResult = await interruptThreadTurnHttp(threadId, turnId);
    } catch (e) {
      const err = e as Error & { status?: number };
      if (err.status !== 409) {
        throw e;
      }
    }
  }
  return interruptResult;
}
