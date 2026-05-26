/**
 * D9 — Two-layer turn stop contract (desktop shell).
 *
 * Layer 1 (`disconnectThreadEventStream`): abort local SSE / poll consumption + `runtime_cancel_sse`.
 * Layer 2 (`interruptThreadTurn` HTTP): `Op::Interrupt` on the runtime — stops LLM/tools.
 *
 * UI **Stop** must call `stopThreadTurn` (layer 2 then layer 1). Layer 1 alone leaves the turn running.
 *
 * SSOT: docs/tech/API_DESIGN.md §2.1.1 · RUNTIME_ARCHITECTURE.md §8
 */

import type { TurnRecord } from './client';
import { interruptThreadTurn as interruptThreadTurnHttp } from './client';

export type { TurnRecord };

/** Layer 1 only — disconnect WebView ↔ sidecar event pipe; does not stop the runtime turn. */
export function disconnectThreadEventStream(streamControl?: AbortController | AbortSignal): void {
  if (streamControl instanceof AbortController) {
    streamControl.abort();
  }
  if (typeof window === 'undefined' || !('__TAURI_INTERNALS__' in window)) {
    return;
  }
  void import('@tauri-apps/api/core')
    .then(({ invoke }) => invoke('runtime_cancel_sse'))
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
  disconnectThreadEventStream(params.streamControl);
  return interruptResult;
}
