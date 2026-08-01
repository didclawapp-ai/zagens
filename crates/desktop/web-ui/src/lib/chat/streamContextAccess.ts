/**
 * Registry accessors for per-thread stream handles (multi-session SSOT).
 * Replaces legacy global `threadTurnRef` / `streamSessionRef` /
 * `liveStreamDeliverRef` / `streamRecoveryContextRef`.
 */
import type { SseTurnEvent } from '../../api/client';
import type { StreamRecoveryContext } from '../../hooks/useTurnStreamRecovery';
import type { FinishOnceOptions, StreamSessionControl } from '../../hooks/useTurnStream';
import type { StreamContextRegistry } from '../../hooks/useStreamContextRegistry';
import { makeEmptyPanelSlice } from '../../hooks/useStreamContextRegistry';
import type { CachedUiMessage } from './sessionUiCache';
import { cacheSessionUiMessages } from './sessionUiCache';
import { isActiveStreamView, lookupThreadIdForSession } from './streamContextStore';

export type ThreadTurnPair = { threadId: string; turnId: string };

const EMPTY_TURN: ThreadTurnPair = { threadId: '', turnId: '' };

export function readThreadTurn(
  registry: StreamContextRegistry | null | undefined,
  threadId: string | null | undefined,
): ThreadTurnPair {
  const tid = threadId?.trim();
  if (!registry || !tid) return EMPTY_TURN;
  return registry.getContext(tid)?.threadTurn ?? { threadId: tid, turnId: '' };
}

export function writeThreadTurn(
  registry: StreamContextRegistry | null | undefined,
  threadId: string,
  turnId: string,
): void {
  const tid = threadId.trim();
  if (!registry || !tid) return;
  registry.ensureContext(tid);
  registry.patchContext(tid, { threadTurn: { threadId: tid, turnId } });
}

export function resolveActiveThreadTurn(
  registry: StreamContextRegistry | null | undefined,
  resumedThreadId: string | null | undefined,
): ThreadTurnPair {
  const tid =
    resumedThreadId?.trim() || registry?.activeThreadIdRef.current?.trim() || '';
  if (!tid) return EMPTY_TURN;
  return readThreadTurn(registry, tid);
}

export function resolveThreadIdForSend(
  registry: StreamContextRegistry | null | undefined,
  resumedThreadId: string | null | undefined,
  activeSessionId: string | null | undefined,
): string | null {
  const fromResume = resumedThreadId?.trim();
  if (fromResume) return fromResume;
  const fromRegistry = registry?.activeThreadIdRef.current?.trim();
  if (fromRegistry) return fromRegistry;
  return lookupThreadIdForSession(
    registry?.contextsRef.current ?? new Map(),
    activeSessionId,
    fromRegistry,
  );
}

export function readStreamSession(
  registry: StreamContextRegistry | null | undefined,
  threadId: string | null | undefined,
): StreamSessionControl | null {
  const tid = threadId?.trim();
  if (!registry || !tid) return null;
  return registry.getContext(tid)?.streamSession ?? null;
}

export function writeStreamSession(
  registry: StreamContextRegistry | null | undefined,
  threadId: string,
  session: StreamSessionControl | null,
): void {
  const tid = threadId.trim();
  if (!registry || !tid) return;
  registry.ensureContext(tid);
  registry.patchContext(tid, { streamSession: session });
}

export function readRecoveryCtx(
  registry: StreamContextRegistry | null | undefined,
  threadId: string | null | undefined,
): StreamRecoveryContext | null {
  const tid = threadId?.trim();
  if (!registry || !tid) return null;
  return registry.getContext(tid)?.recoveryCtx ?? null;
}

export function writeRecoveryCtx(
  registry: StreamContextRegistry | null | undefined,
  threadId: string,
  ctx: StreamRecoveryContext | null,
): void {
  const tid = threadId.trim();
  if (!registry || !tid) return;
  registry.ensureContext(tid);
  registry.patchContext(tid, { recoveryCtx: ctx });
}

export function readLiveDeliver(
  registry: StreamContextRegistry | null | undefined,
  threadId: string | null | undefined,
): ((ev: SseTurnEvent, filter?: { turnId: string }) => void) | null {
  const tid = threadId?.trim();
  if (!registry || !tid) return null;
  return registry.getContext(tid)?.liveDeliver ?? null;
}

export function writeLiveDeliver(
  registry: StreamContextRegistry | null | undefined,
  threadId: string,
  deliver:
    | ((ev: SseTurnEvent, filter?: { turnId: string }) => void)
    | null,
): void {
  const tid = threadId.trim();
  if (!registry || !tid) return;
  registry.ensureContext(tid);
  registry.patchContext(tid, { liveDeliver: deliver });
}

/** Clear ephemeral stream handles after a turn ends (keep messages / panelSlice).
 *
 * Note: `AbortController` is managed by `useTurnStream.streamControllersRef`,
 * not the registry, so it is not touched here.
 */
export function compactIdleStreamHandles(
  registry: StreamContextRegistry | null | undefined,
  threadId: string,
): void {
  const tid = threadId.trim();
  if (!registry || !tid) return;
  if (!registry.getContext(tid)) return;
  registry.patchContext(tid, {
    streamSession: null,
    liveDeliver: null,
    recoveryCtx: null,
    pendingApproval: null,
    isStreaming: false,
    // Drop the finished turn's timeline so the next turn on this thread cannot
    // rehydrate stale blocks into its fresh stream (duplicate output guard).
    timelineState: undefined,
  });
}

export function clearActiveStreamHandles(
  registry: StreamContextRegistry | null | undefined,
  threadId: string | null | undefined,
): void {
  const tid = threadId?.trim();
  if (!tid) return;
  compactIdleStreamHandles(registry, tid);
}

export function resolveEventDeliver(
  registry: StreamContextRegistry | null | undefined,
  activeThreadId: string | null | undefined,
): ((ev: SseTurnEvent, filter?: { turnId: string }) => void) | null {
  const tid = activeThreadId?.trim();
  if (!tid) return null;
  return (
    readRecoveryCtx(registry, tid)?.deliverSseEvent ??
    readLiveDeliver(registry, tid) ??
    null
  );
}

export function hasAnyActiveStreamHandle(
  registry: StreamContextRegistry | null | undefined,
  activeThreadId: string | null | undefined,
): boolean {
  const tid = activeThreadId?.trim();
  if (!tid) return false;
  const ctx = registry?.getContext(tid);
  return !!(ctx?.recoveryCtx || ctx?.liveDeliver || ctx?.isStreaming);
}

export function patchRecoveryAssistantId(
  registry: StreamContextRegistry | null | undefined,
  threadId: string,
  assistantId: string,
): void {
  const tid = threadId.trim();
  if (!registry || !tid) return;
  const prev = readRecoveryCtx(registry, tid);
  if (!prev) return;
  registry.patchContext(tid, { recoveryCtx: { ...prev, assistantId } });
}

export function invokeFinishOnce(
  registry: StreamContextRegistry | null | undefined,
  threadId: string | null | undefined,
  options?: FinishOnceOptions,
): void {
  const tid = threadId?.trim();
  if (!tid) return;
  readRecoveryCtx(registry, tid)?.finishOnce(options);
  readStreamSession(registry, tid)?.finishOnce(options);
}

/** Idle contexts older than this may have messages/panelSlice evicted (S0.2). */
export const IDLE_CONTEXT_EVICT_MS = 5 * 60 * 1000;

/**
 * Evict heavy fields from an idle, non-active context after `IDLE_CONTEXT_EVICT_MS`.
 * Messages are cached to `sessionUiCache` first; threadTurn/sessionId are kept for reattach.
 */
export function evictIdleContextMessages(
  registry: StreamContextRegistry | null | undefined,
  threadId: string,
  activeThreadId: string | null | undefined,
  sessionUiCache: Map<string, CachedUiMessage[]>,
  now = Date.now(),
): boolean {
  const tid = threadId.trim();
  if (!registry || !tid) return false;
  const ctx = registry.getContext(tid);
  if (!ctx) return false;
  if (ctx.isStreaming) return false;
  if (isActiveStreamView(activeThreadId ?? null, tid)) return false;
  const lastAt = ctx.lastActivityAt ?? 0;
  if (now - lastAt < IDLE_CONTEXT_EVICT_MS) return false;
  if (ctx.messages.length === 0 && ctx.panelSlice.checklist == null &&
      ctx.panelSlice.taskGraph == null && ctx.panelSlice.context == null &&
      ctx.panelSlice.scratchpad == null && ctx.panelSlice.lhtChip == null) {
    return false;
  }

  const sessionId = ctx.sessionId?.trim();
  if (sessionId && ctx.messages.length > 0) {
    cacheSessionUiMessages(sessionUiCache, sessionId, ctx.messages);
  }

  registry.patchContext(tid, {
    messages: [],
    panelSlice: makeEmptyPanelSlice(),
  });
  return true;
}
