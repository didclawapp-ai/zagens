import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from 'react';
import { getThreadDetail, threadTurnStillActive } from '../api/client';
import { disconnectThreadEventStream, stopThreadTurn } from '../api/turnControl';
import { applyOptimisticThreadStop } from '../lib/chat/threadStatusStore';
import { toast } from '../lib/toast';
import {
  readStreamSession,
  readThreadTurn,
  resolveActiveThreadTurn,
  writeThreadTurn,
} from '../lib/chat/streamContextAccess';
import type { StreamContextRegistry } from './useStreamContextRegistry';
import { useActiveThreadIds } from './useThreadStatusStore';

export type FinishOnceOptions = {
  /** Skip backend active-turn re-lock (user Stop or local start failure). */
  force?: boolean;
  /** Trust `turn.completed` / `done` — finish UI without re-locking on active-turn poll. */
  terminal?: boolean;
};

export type StreamSessionControl = {
  markInterrupted: () => void;
  finishOnce: (options?: FinishOnceOptions) => void;
};

export type UseTurnStreamParams = {
  resumedThreadId: string | null;
  streamingRef: MutableRefObject<boolean>;
  streamRegistry: StreamContextRegistry;
  t: (key: string, params?: Record<string, string>) => string;
  onCancelSideEffects: () => void;
  /** Clears detach/recovery state before runtime interrupt (wired by useTurnSend). */
  cancelCleanupRef?: MutableRefObject<(() => void) | null>;
};

export type UseTurnStreamResult = {
  /** Active thread ids from authoritative `threadStatusStore` (P3). */
  activeThreadIds: Set<string>;
  pendingComposerStream: boolean;
  setPendingComposerStream: Dispatch<SetStateAction<boolean>>;
  streaming: boolean;
  streamingRef: MutableRefObject<boolean>;
  streamControllersRef: MutableRefObject<Map<string, AbortController>>;
  /**
   * Tracks the stream-key of the current pending send (before `turn_started`
   * resolves a real threadId). Used by `handleCancelStream` to find the right
   * AbortController for a brand-new session cancel, without falling back to a
   * shared `__pending__` key that might belong to another send.
   * Set by `useTurnSend.handleSend`, cleared on `turn_started` / completion.
   */
  pendingSendKeyRef: MutableRefObject<string | null>;
  /** Set synchronously on user Stop — `finishOnce` must not re-lock while interrupt is in flight. */
  userStopRequestedRef: MutableRefObject<boolean>;
  abortThreadStream: (threadId: string | null | undefined) => void;
  handleCancelStream: () => void;
};

export function useTurnStream({
  resumedThreadId,
  streamingRef,
  streamRegistry,
  t,
  onCancelSideEffects,
  cancelCleanupRef,
}: UseTurnStreamParams): UseTurnStreamResult {
  const activeThreadIds = useActiveThreadIds();
  const [pendingComposerStream, setPendingComposerStream] = useState(false);

  // Composer lock applies only to the active view's thread — background streams
  // must not disable input on a different session (multi-session P0.4).
  const streaming = useMemo(() => {
    const tid = resumedThreadId?.trim() ?? '';
    if (!tid) {
      return pendingComposerStream;
    }
    return activeThreadIds.has(tid) || pendingComposerStream;
  }, [activeThreadIds, pendingComposerStream, resumedThreadId]);

  const streamControllersRef = useRef<Map<string, AbortController>>(new Map());
  const pendingSendKeyRef = useRef<string | null>(null);
  const userStopRequestedRef = useRef(false);

  useEffect(() => {
    streamingRef.current = streaming;
  }, [streaming, streamingRef]);

  const abortThreadStream = useCallback(
    (threadId: string | null | undefined, opts?: { clearComposerLock?: boolean }) => {
      if (!threadId) return;
      streamControllersRef.current.get(threadId)?.abort();
      streamControllersRef.current.delete(threadId);
      if (opts?.clearComposerLock !== false) {
        setPendingComposerStream(false);
      }
    },
    [],
  );

  const onCancelSideEffectsRef = useRef(onCancelSideEffects);
  onCancelSideEffectsRef.current = onCancelSideEffects;

  const handleCancelStream = useCallback(() => {
    userStopRequestedRef.current = true;
    cancelCleanupRef?.current?.();

    const activeTurn = resolveActiveThreadTurn(streamRegistry, resumedThreadId);
    const threadId = activeTurn.threadId || resumedThreadId || '';
    const turnId = activeTurn.turnId;

    const pendingKey = pendingSendKeyRef.current;
    const streamControl =
      (threadId ? streamControllersRef.current.get(threadId) : undefined) ??
      (pendingKey ? streamControllersRef.current.get(pendingKey) : undefined) ??
      streamControllersRef.current.get('__pending__') ??
      undefined;

    if (threadId) {
      applyOptimisticThreadStop(threadId, turnId || undefined);
    }
    disconnectThreadEventStream(streamControl, threadId || undefined);

    const session = readStreamSession(streamRegistry, threadId);
    if (session) {
      session.markInterrupted();
      session.finishOnce({ force: true });
    } else {
      abortThreadStream(threadId || resumedThreadId);
    }

    void (async () => {
      let resolvedTurnId = turnId;
      if (threadId && !resolvedTurnId.trim()) {
        try {
          const detail = await getThreadDetail(threadId);
          const latest = detail.thread.latest_turn_id ?? '';
          if (latest && (await threadTurnStillActive(threadId, latest))) {
            resolvedTurnId = latest;
            writeThreadTurn(streamRegistry, threadId, latest);
          }
        } catch {
          /* best-effort — local UI already stopped */
        }
      }

      try {
        await stopThreadTurn({ threadId, turnId: resolvedTurnId, streamControl });
      } catch (e) {
        const err = e as Error & { status?: number };
        if (err.status !== 409) {
          toast.warning(t('composer.interruptFailed', { message: err.message || String(e) }));
        }
      }

      onCancelSideEffectsRef.current();
    })();
  }, [
    abortThreadStream,
    cancelCleanupRef,
    resumedThreadId,
    streamControllersRef,
    streamRegistry,
    t,
  ]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || e.ctrlKey || e.metaKey || e.altKey) return;
      const tag = (e.target as HTMLElement)?.tagName?.toLowerCase();
      if (tag === 'input' || tag === 'textarea' || tag === 'select') return;
      if (!streamingRef.current) return;
      e.preventDefault();
      handleCancelStream();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [handleCancelStream]);

  return {
    activeThreadIds,
    pendingComposerStream,
    setPendingComposerStream,
    streaming,
    streamingRef,
    streamControllersRef,
    pendingSendKeyRef,
    userStopRequestedRef,
    abortThreadStream,
    handleCancelStream,
  };
}
