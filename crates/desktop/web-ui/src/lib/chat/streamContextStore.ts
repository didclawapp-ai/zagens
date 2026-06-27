/**
 * Pure Map helpers for per-thread stream contexts (multi-session P0.2).
 * Used by `useStreamContextRegistry` and `multiSession.test.ts`.
 */
import {
  makeEmptyContext,
  type StreamContext,
} from '../../hooks/useStreamContextRegistry';
import { getActiveThreadIdsFromStore } from './threadStatusStore';
import type { TurnChatMessage } from '../../hooks/useTurnSend';

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

/** Pre-`turn_started` draft bucket for a persisted session row. */
export function draftContextKey(sessionId: string): string {
  return `__draft__:${sessionId.trim()}`;
}

/** Brand-new composer before any session id exists. */
export const NEW_SESSION_DRAFT_KEY = '__new__';

/** Registry key for the active view transcript (thread > session draft > new-session draft). */
export function resolveViewMessageKey(
  threadId: string | null | undefined,
  sessionId: string | null | undefined,
): string {
  const tid = threadId?.trim();
  if (tid) return tid;
  const sid = sessionId?.trim();
  if (sid) return draftContextKey(sid);
  return NEW_SESSION_DRAFT_KEY;
}

export function getViewMessagesFromMap(
  map: Map<string, StreamContext>,
  threadId: string | null | undefined,
  sessionId: string | null | undefined,
): TurnChatMessage[] {
  const key = resolveViewMessageKey(threadId, sessionId);
  return map.get(key)?.messages ?? [];
}

/** Move pre-turn_started draft messages onto the runtime thread context. */
export function migrateDraftContextInMap(
  map: Map<string, StreamContext>,
  sessionId: string | null | undefined,
  threadId: string,
): boolean {
  const tid = threadId.trim();
  if (!tid) return false;
  const draftKeys = [
    ...(sessionId?.trim() ? [draftContextKey(sessionId)] : []),
    NEW_SESSION_DRAFT_KEY,
  ];
  let draft: StreamContext | undefined;
  let draftKey: string | undefined;
  for (const key of draftKeys) {
    const candidate = map.get(key);
    if (candidate && candidate.messages.length > 0) {
      draft = candidate;
      draftKey = key;
      break;
    }
  }
  if (!draft || !draftKey) return false;
  const prev = map.get(tid) ?? makeEmptyContext(tid, sessionId ?? draft.sessionId);
  map.set(tid, {
    ...prev,
    messages: draft.messages,
    sessionId: sessionId ?? prev.sessionId ?? draft.sessionId,
    isStreaming: prev.isStreaming || draft.isStreaming,
    threadTurn: prev.threadTurn.threadId === tid ? prev.threadTurn : { threadId: tid, turnId: '' },
  });
  map.delete(draftKey);
  return true;
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

/** Resolve a runtime thread id already bound to a persisted session. */
export function lookupThreadIdForSession(
  contexts: Map<string, { sessionId: string | null }>,
  sessionId: string | null | undefined,
  activeThreadId?: string | null,
): string | null {
  const sid = sessionId?.trim();
  if (!sid) return null;
  for (const [tid, ctx] of contexts) {
    if (ctx.sessionId === sid) return tid;
  }
  const active = activeThreadId?.trim();
  if (active && contexts.has(active)) return active;
  return null;
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

type StreamContextStreamProbe = Pick<StreamContext, 'isStreaming' | 'messages' | 'sessionId'>;

/** Thread or draft context still has an in-flight assistant turn. */
export function contextHasActiveStream(
  ctx: StreamContextStreamProbe | undefined,
): boolean {
  if (!ctx) return false;
  if (ctx.isStreaming === true) return true;
  return ctx.messages.some((m) => m.role === 'assistant' && m.isStreaming === true);
}

export type CollectStreamingSessionIdsOptions = {
  /** Authoritative active thread ids from `threadStatusStore` (P3). */
  activeThreadIds: Iterable<string>;
  contexts: Map<string, StreamContext>;
  activeSessionId: string | null | undefined;
  resumedThreadId: string | null | undefined;
  activeThreadId: string | null | undefined;
  /** Brand-new session gap before `turn_started` / store status. */
  pendingComposerStream: boolean;
};

function resolveContextSessionId(
  threadId: string,
  ctx: StreamContextStreamProbe,
  activeSessionId: string | null | undefined,
  resumedThreadId: string | null | undefined,
): string | null {
  if (threadId.startsWith('__draft__:')) {
    return threadId.slice('__draft__:'.length) || null;
  }
  if (threadId === NEW_SESSION_DRAFT_KEY) {
    return activeSessionId?.trim() ?? null;
  }
  const sid = ctx.sessionId?.trim();
  if (sid) return sid;
  if (threadId === resumedThreadId?.trim()) {
    return activeSessionId?.trim() ?? null;
  }
  return null;
}

/** Session ids to show the SessionStrip in-flight spinner (multi-session P0.4). */
export function collectStreamingSessionIds(
  options: CollectStreamingSessionIdsOptions,
): Set<string> {
  const ids = new Set<string>();
  const evaluatedThreads = new Set<string>();
  const activeSid = options.activeSessionId?.trim() ?? null;
  const resumedTid = options.resumedThreadId?.trim() ?? null;

  const maybeAdd = (sessionId: string | null) => {
    const sid = sessionId?.trim();
    if (sid) ids.add(sid);
  };

  for (const raw of options.activeThreadIds) {
    const tid = raw.trim();
    if (!tid || evaluatedThreads.has(tid)) continue;
    evaluatedThreads.add(tid);
    const ctx = options.contexts.get(tid);
    const resolvedCtx = ctx ?? { isStreaming: false, messages: [], sessionId: null };
    maybeAdd(resolveContextSessionId(tid, resolvedCtx, activeSid, resumedTid));
  }

  if (options.pendingComposerStream && activeSid && !ids.has(activeSid)) {
    const draftKey = draftContextKey(activeSid);
    const draftCtx =
      options.contexts.get(draftKey) ?? options.contexts.get(NEW_SESSION_DRAFT_KEY);
    if (contextHasActiveStream(draftCtx) || !resumedTid) {
      maybeAdd(activeSid);
    }
  }

  return ids;
}

/** Thread ids that may need periodic replay reconcile (P3: probe-only, store-driven). */
export function collectReconcileThreadIds(activeThreadId: string | null): string[] {
  const ids = new Set<string>(getActiveThreadIdsFromStore());
  const active = activeThreadId?.trim();
  if (active) ids.add(active);
  return [...ids];
}
