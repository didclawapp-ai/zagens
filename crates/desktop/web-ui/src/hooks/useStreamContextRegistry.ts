/**
 * Per-session stream context registry (multi-session P0.2).
 *
 * Holds one `StreamContext` per thread so that multiple sessions can stream
 * concurrently. All per-thread handles (`threadTurn`, `streamSession`,
 * `liveDeliver`, `recoveryCtx`, `messages`, `panelSlice`) live in the registry.
 * non-active contexts keep receiving their SSE events into their own
 * `messages` / `panelSlice` so the user can switch back to live progress.
 *
 * Migration policy (see `doc_Private/docs/desktop/multi-session-streaming-plan.md`
 * §8.2): registry is the SSOT for per-thread messages. `App.tsx` derives the
 * active view transcript via `getViewMessages`; `createSetMessagesForView` routes
 * all `setMessages` calls into the registry (draft bucket pre-`turn_started`).
 */

import { useCallback, useRef, useState } from 'react';
import type { MutableRefObject, Dispatch, SetStateAction } from 'react';
import {
  deleteContextFromMap,
  ensureContextInMap,
  getViewMessagesFromMap,
  isActiveStreamView as isActiveStreamViewPure,
  migrateDraftContextInMap,
  patchContextInMap,
  resolveViewMessageKey,
} from '../lib/chat/streamContextStore';
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

/** One streaming session's isolated state.
 *
 * Note: `AbortController` is NOT stored here. It lives in
 * `useTurnStream.streamControllersRef` (a `Map<threadId, AbortController>`)
 * which is the SSOT for stream cancellation. Keeping it out of the registry
 * avoids redundant re-renders on every controller mutation.
 */
export type StreamContext = {
  threadId: string;
  sessionId: string | null;
  messages: TurnChatMessage[];
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
  /** Active view transcript (derived from thread or pre-turn_started draft). */
  getViewMessages: (
    threadId: string | null | undefined,
    sessionId: string | null | undefined,
  ) => TurnChatMessage[];
  /** Move session/new-session draft messages onto a runtime thread. */
  migrateDraftToThread: (
    sessionId: string | null | undefined,
    threadId: string,
  ) => void;
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
      const { ctx, changed } = ensureContextInMap(
        contextsRef.current,
        threadId,
        sessionId,
      );
      if (changed) bump();
      return ctx;
    },
    [bump],
  );

  const patchContext = useCallback(
    (
      threadId: string,
      patch: Partial<StreamContext> | ((prev: StreamContext) => Partial<StreamContext>),
    ) => {
      if (patchContextInMap(contextsRef.current, threadId, patch)) {
        bump();
      }
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
      const { deleted, nextActiveThreadId } = deleteContextFromMap(
        contextsRef.current,
        threadId,
        activeThreadIdRef.current,
      );
      if (!deleted) return;
      if (nextActiveThreadId !== activeThreadIdRef.current) {
        setActiveThreadId(nextActiveThreadId);
      }
      bump();
    },
    [bump, setActiveThreadId],
  );

  const isActiveStreamView = useCallback(
    (threadId: string | null | undefined) =>
      isActiveStreamViewPure(activeThreadIdRef.current, threadId),
    [],
  );

  const getViewMessages = useCallback(
    (threadId: string | null | undefined, sessionId: string | null | undefined) =>
      getViewMessagesFromMap(contextsRef.current, threadId, sessionId),
    [],
  );

  const migrateDraftToThread = useCallback(
    (sessionId: string | null | undefined, threadId: string) => {
      if (migrateDraftContextInMap(contextsRef.current, sessionId, threadId)) {
        bump();
      }
    },
    [bump],
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
    getViewMessages,
    migrateDraftToThread,
  };
}

/**
 * Registry-backed `setMessages` for the active view. Resolves the target bucket
 * (runtime thread, session draft, or brand-new session draft) and bumps version.
 */
export function createSetMessagesForView(
  registry: StreamContextRegistry,
  getViewPointers: () => {
    threadId: string | null | undefined;
    sessionId: string | null | undefined;
  },
): Dispatch<SetStateAction<TurnChatMessage[]>> {
  return (next: TurnChatMessage[] | ((prev: TurnChatMessage[]) => TurnChatMessage[])) => {
    const { threadId, sessionId } = getViewPointers();
    const key = resolveViewMessageKey(threadId, sessionId);
    registry.ensureContext(key, sessionId ?? null);
    const ctx = registry.getContext(key)!;
    const resolved =
      typeof next === 'function'
        ? (next as (p: TurnChatMessage[]) => TurnChatMessage[])(ctx.messages)
        : next;
    registry.setMessages(key, resolved);
  };
}
