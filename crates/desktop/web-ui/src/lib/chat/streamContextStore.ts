/**
 * Pure Map helpers for per-thread stream contexts (multi-session P0.2).
 * Used by `useStreamContextRegistry` and `multiSession.selfcheck.ts`.
 */
import {
  makeEmptyContext,
  type StreamContext,
} from '../../hooks/useStreamContextRegistry';

export function ensureContextInMap(
  map: Map<string, StreamContext>,
  threadId: string,
  sessionId?: string | null,
): { ctx: StreamContext; changed: boolean } {
  const tid = threadId.trim();
  let ctx = map.get(tid);
  if (!ctx) {
    ctx = makeEmptyContext(tid, sessionId ?? null);
    map.set(tid, ctx);
    return { ctx, changed: true };
  }
  if (sessionId != null && ctx.sessionId !== sessionId) {
    ctx = { ...ctx, sessionId };
    map.set(tid, ctx);
    return { ctx, changed: true };
  }
  return { ctx, changed: false };
}

export function patchContextInMap(
  map: Map<string, StreamContext>,
  threadId: string,
  patch: Partial<StreamContext> | ((prev: StreamContext) => Partial<StreamContext>),
): boolean {
  const tid = threadId.trim();
  const prev = map.get(tid);
  if (!prev) return false;
  const delta = typeof patch === 'function' ? patch(prev) : patch;
  map.set(tid, { ...prev, ...delta });
  return true;
}

export function deleteContextFromMap(
  map: Map<string, StreamContext>,
  threadId: string,
  activeThreadId: string | null,
): { deleted: boolean; nextActiveThreadId: string | null } {
  const tid = threadId.trim();
  const deleted = map.delete(tid);
  if (!deleted) {
    return { deleted: false, nextActiveThreadId: activeThreadId };
  }
  return {
    deleted: true,
    nextActiveThreadId: activeThreadId === tid ? null : activeThreadId,
  };
}

export function isActiveStreamView(
  activeThreadId: string | null,
  threadId: string | null | undefined,
): boolean {
  if (!threadId) return true;
  return activeThreadId === threadId;
}

/**
 * Whether an SSE event for `eventThreadId` should update a background
 * `StreamContext` instead of the active view transcript.
 *
 * `pendingSend` is true for the SSE consumer that initiated the current
 * `handleSend` until its `turn_started` is processed. That keeps a brand-new
 * session on the active path even when `activeThreadId` is still null.
 */
export function isBackgroundStreamEvent(
  activeThreadId: string | null,
  eventThreadId: string | null | undefined,
  ownerThreadId: string | null,
  pendingSend = false,
): boolean {
  if (!eventThreadId) return false;
  if (isActiveStreamView(activeThreadId, eventThreadId)) return false;
  if (pendingSend && (ownerThreadId == null || ownerThreadId === eventThreadId)) {
    return false;
  }
  return true;
}

/** Immutable remove from streaming set; returns null when unchanged. */
export function removeThreadFromStreamingSet(
  prev: Set<string>,
  threadId: string,
): Set<string> | null {
  const tid = threadId.trim();
  if (!prev.has(tid)) return null;
  const next = new Set(prev);
  next.delete(tid);
  return next;
}

/** Thread ids that may need periodic replay reconcile (P1 multi-session). */
export function collectReconcileThreadIds(
  streamingThreadIds: Iterable<string>,
  activeThreadId: string | null,
): string[] {
  const ids = new Set<string>();
  for (const raw of streamingThreadIds) {
    const tid = raw.trim();
    if (tid) ids.add(tid);
  }
  const active = activeThreadId?.trim();
  if (active) ids.add(active);
  return [...ids];
}
