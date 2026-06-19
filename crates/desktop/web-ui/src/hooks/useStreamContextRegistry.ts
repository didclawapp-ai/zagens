/**
 * Per-session stream context registry (multi-session P0.2).
 *
 * Holds one `StreamContext` per thread so that multiple sessions can stream
 * concurrently without sharing a single `messages` state, `threadTurnRef`,
 * or panel slice. The active view is just a pointer (`activeThreadId`);
 * non-active contexts keep receiving their SSE events into their own
 * `messages` / `panelSlice` so the user can switch back to live progress.
 *
 * Migration policy (see `doc_Private/docs/desktop/multi-session-streaming-plan.md`
 * §8.2): this registry is introduced incrementally. Legacy single-instance
 * refs (`threadTurnRef`, `streamSessionRef`, `liveStreamDeliverRef`,
 * `streamRecoveryContextRef`) remain the source of truth for paths that have
 * not been migrated yet; `useStreamContextRegistry` is wired into `useTurnSend`
 * kind-by-kind. Once all writes go through the registry, the legacy refs are
 * removed.
 *
 * The registry is intentionally framework-light: it owns a `Map` in a ref plus
 * a version counter in state to trigger re-renders, and exposes stable
 * callbacks. `App.tsx` derives `messages` for the active view from
 * `getContext(activeThreadId)?.messages`.
 */

import { useCallback, useRef, useState } from 'react';
import type { MutableRefObject, Dispatch, SetStateAction } from 'react';
import type { ApprovalState } from '../hooks/useTurnApproval';
import type { FinishOnceOptions, StreamSessionControl } from '../hooks/useTurnStream';
import type { StreamRecoveryContext } from '../hooks/useTurnStreamRecovery';
import type { TurnChatMessage } from '../hooks/useTurnSend';
import type { SseTurnEvent } from '../api/client';
import type { ThreadContextSnapshot } from '../lib/contextUsage';
import type { HarnessTaskGraph } from '../lib/types/longHorizon';
import type { ScratchpadStatus } from '../api/client';
import type { LhtChipState } from '../lib/lhtChip';

/** Per-thread snapshot of the right-inspector panel state (P0.6). */
export type PanelSlice = {
  checklist: unknown | null;
  taskGraph: HarnessTaskGraph | null;
  context: ThreadContextSnapshot | null;
  scratchpad: ScratchpadStatus | null;
  lhtChip: LhtChipState | null;
};

/** One streaming session's isolated state. */
export type StreamContext = {
  threadId: string;
  sessionId: string | null;
  messages: TurnChatMessage[];
  controller: AbortController | null;
  threadTurn: { threadId: string; turnId: string };
  streamSession: StreamSessionControl | null;
  liveDeliver: ((ev: SseTurnEvent, filter?: { turnId: string }) => void) | null;
  recoveryCtx: StreamRecoveryContext | null;
  panelSlice: PanelSlice;
  pendingApproval: ApprovalState | null;
  isStreaming: boolean;
};

export function makeEmptyPanelSlice(): PanelSlice {
  return {
    checklist: null,
    taskGraph: null,
    context: null,
    scratchpad: null,
    lhtChip: null,
  };
}

export function makeEmptyContext(threadId: string, sessionId: string | null): StreamContext {
  return {
    threadId,
    sessionId,
    messages: [],
    controller: null,
    threadTurn: { threadId, turnId: '' },
    streamSession: null,
    liveDeliver: null,
    recoveryCtx: null,
    panelSlice: makeEmptyPanelSlice(),
    pendingApproval: null,
    isStreaming: false,
  };
}

export type StreamContextRegistry = {
  /** All contexts keyed by threadId. Re-render on change via `version`. */
  contexts: Map<string, StreamContext>;
  /** The thread whose messages/panel are currently rendered in the active view. */
  activeThreadId: string | null;
  setActiveThreadId: (threadId: string | null) => void;
  /** Synchronous read for closures (no re-render trigger). */
  contextsRef: MutableRefObject<Map<string, StreamContext>>;
  activeThreadIdRef: MutableRefObject<string | null>;
  getContext: (threadId: string | null | undefined) => StreamContext | undefined;
  ensureContext: (threadId: string, sessionId?: string | null) => StreamContext;
  /** Patch a context; pass a partial or a producer. Bumps version → re-render. */
  patchContext: (
    threadId: string,
    patch: Partial<StreamContext> | ((prev: StreamContext) => Partial<StreamContext>),
  ) => void;
  /** Replace a context's messages (convenience for the common `setMessages` pattern). */
  setMessages: (threadId: string, messages: TurnChatMessage[]) => void;
  deleteContext: (threadId: string) => void;
  /** Is `threadId` the active view AND not in a background-only state? */
  isActiveStreamView: (threadId: string | null | undefined) => boolean;
  /** Bumped on every mutation so consumers re-render. */
  version: number;
};

export function useStreamContextRegistry(): StreamContextRegistry {
  const [version, setVersion] = useState(0);
  const [activeThreadId, setActiveThreadIdState] = useState<string | null>(null);
  const contextsRef = useRef<Map<string, StreamContext>>(new Map());
  const activeThreadIdRef = useRef<string | null>(null);

  const bump = useCallback(() => setVersion((v) => v + 1), []);

  const setActiveThreadId = useCallback(
    (threadId: string | null) => {
      activeThreadIdRef.current = threadId;
      setActiveThreadIdState(threadId);
    },
    [],
  );

  const getContext = useCallback((threadId: string | null | undefined) => {
    if (!threadId) return undefined;
    return contextsRef.current.get(threadId);
  }, []);

  const ensureContext = useCallback(
    (threadId: string, sessionId?: string | null): StreamContext => {
      const tid = threadId.trim();
      let ctx = contextsRef.current.get(tid);
      if (!ctx) {
        ctx = makeEmptyContext(tid, sessionId ?? null);
        contextsRef.current.set(tid, ctx);
        bump();
      } else if (sessionId != null && ctx.sessionId !== sessionId) {
        ctx = { ...ctx, sessionId };
        contextsRef.current.set(tid, ctx);
        bump();
      }
      return ctx;
    },
    [bump],
  );

  const patchContext = useCallback(
    (
      threadId: string,
      patch: Partial<StreamContext> | ((prev: StreamContext) => Partial<StreamContext>),
    ) => {
      const tid = threadId.trim();
      const prev = contextsRef.current.get(tid);
      if (!prev) return;
      const delta = typeof patch === 'function' ? patch(prev) : patch;
      contextsRef.current.set(tid, { ...prev, ...delta });
      bump();
    },
    [bump],
  );

  const setMessagesForThread = useCallback(
    (threadId: string, messages: TurnChatMessage[]) => {
      patchContext(threadId, { messages });
    },
    [patchContext],
  );

  const deleteContext = useCallback(
    (threadId: string) => {
      const tid = threadId.trim();
      if (contextsRef.current.delete(tid)) {
        if (activeThreadIdRef.current === tid) {
          setActiveThreadId(null);
        }
        bump();
      }
    },
    [bump, setActiveThreadId],
  );

  const isActiveStreamView = useCallback(
    (threadId: string | null | undefined) => {
      if (!threadId) return true; // threadless events (global status) default through
      return activeThreadIdRef.current === threadId;
    },
    [],
  );

  return {
    contexts: contextsRef.current,
    activeThreadId,
    setActiveThreadId,
    contextsRef,
    activeThreadIdRef,
    getContext,
    ensureContext,
    patchContext,
    setMessages: setMessagesForThread,
    deleteContext,
    isActiveStreamView,
    version,
  };
}

/**
 * Bridge a legacy single-instance ref setter (e.g. `setMessages`) into the
 * registry for the active thread. Used during the P0.2→P0.3 migration so
 * unmigrated paths that still call `setMessages(prev => ...)` transparently
 * land in the active context's messages.
 */
export function bindLegacySetMessages(
  registry: StreamContextRegistry,
  setMessagesState: Dispatch<SetStateAction<TurnChatMessage[]>>,
) {
  return (next: TurnChatMessage[] | ((prev: TurnChatMessage[]) => TurnChatMessage[])) => {
    const tid = registry.activeThreadIdRef.current;
    if (tid) {
      const ctx = registry.ensureContext(tid);
      const resolved =
        typeof next === 'function' ? (next as (p: TurnChatMessage[]) => TurnChatMessage[])(ctx.messages) : next;
      registry.setMessages(tid, resolved);
      setMessagesState(resolved);
    } else {
      setMessagesState(next as TurnChatMessage[] | ((prev: TurnChatMessage[]) => TurnChatMessage[]));
    }
  };
}
