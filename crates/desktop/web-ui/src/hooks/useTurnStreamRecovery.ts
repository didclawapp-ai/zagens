import {
  useCallback,
  useEffect,
  useRef,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from 'react';
import {
  disconnectThreadEventStream,
  stopThreadTurn,
} from '../api/turnControl';
import {
  getThreadDetail,
  pollThreadTurnEvents,
  threadTurnStillActive,
  type SseTurnEvent,
} from '../api/client';
import { getRuntimeBase, type RuntimeConnectionState } from '../api/client';
import { subscribeCurrentWebviewEvent } from '../lib/tauriListen';
import { dispatchSidecarReadyForPanels } from '../lib/sidecarPanelRecovery';
import { RUNTIME_TRANSIENT_TAG, toast } from '../lib/toast';
import {
  anyAssistantStreaming,
  clearStreamingAssistants,
  lastAssistantMessageId,
  markLastAssistantStreaming,
  rebindStreamingAssistant,
} from '../lib/chat/activeTurnStreamUi';
import { rebuildMessagesFromThreadEvents } from '../lib/chat/rebuildMessagesFromThread';
import type { FinishOnceOptions, StreamSessionControl } from './useTurnStream';
import type { TurnChatMessage } from './useTurnSend';

/** Why the live SSE consumer was detached (backend turn may still run). */
export type StreamDetachReason = 'sidecar_restart' | 'runtime_offline';

export const TURN_DETACHED_TAG = 'turn-detached';

/** After this long offline while detached, auto-interrupt the runtime turn (API billing guard). */
export const OFFLINE_AUTO_INTERRUPT_MS = 120_000;

/** Warn the user that billing may continue before auto-interrupt. */
export const OFFLINE_BILLING_WARN_MS = 15_000;

/** When chat SSE handler is gone but the backend turn is still active, refresh transcript from replay. */
export const ACTIVE_TURN_CHAT_RECONCILE_MS = 8_000;

export type StreamRecoveryContext = {
  assistantId: string;
  threadId: string;
  turnId: string;
  deliverSseEvent: (ev: SseTurnEvent, filter?: { turnId: string }) => void;
  finishOnce: (options?: FinishOnceOptions) => void;
};

export type UseTurnStreamRecoveryParams = {
  t: (key: string, params?: Record<string, string>) => string;
  desktopHost: boolean;
  runtimeConn: RuntimeConnectionState;
  streamingRef: MutableRefObject<boolean>;
  resumedThreadIdRef: MutableRefObject<string | null>;
  threadTurnRef: MutableRefObject<{ threadId: string; turnId: string }>;
  streamControllersRef: MutableRefObject<Map<string, AbortController>>;
  streamSessionRef: MutableRefObject<StreamSessionControl | null>;
  streamRecoveryContextRef: MutableRefObject<StreamRecoveryContext | null>;
  liveStreamDeliverRef: MutableRefObject<
    ((ev: SseTurnEvent, filter?: { turnId: string }) => void) | null
  >;
  setMessages: Dispatch<SetStateAction<TurnChatMessage[]>>;
  setStreamingThreadIds: Dispatch<SetStateAction<Set<string>>>;
  setPendingComposerStream: Dispatch<SetStateAction<boolean>>;
  handleCancelStream: () => void;
  notifyRuntimeTransient: (message: string) => void;
  refreshThreadContext: (threadId: string) => Promise<void>;
};

export type UseTurnStreamRecoveryResult = {
  detachReasonRef: MutableRefObject<StreamDetachReason | null>;
  /** Call from stream `AbortError` — returns true when finishOnce must be skipped. */
  shouldSkipFinishOnAbort: () => boolean;
  detachActiveStream: (reason: StreamDetachReason) => void;
  tryRecoverDetachedTurn: () => Promise<void>;
  /** Clears detach timers/toasts (e.g. user pressed Stop). */
  clearDetachedState: () => void;
};

function appendDetachBanner(content: string, banner: string): string {
  const trimmed = content.trim();
  if (!trimmed) return banner;
  if (trimmed.includes(banner)) return content;
  return `[${banner}] ${content}`;
}

export function useTurnStreamRecovery({
  t,
  desktopHost,
  runtimeConn,
  streamingRef,
  resumedThreadIdRef,
  threadTurnRef,
  streamControllersRef,
  streamSessionRef,
  streamRecoveryContextRef,
  liveStreamDeliverRef,
  setMessages,
  setStreamingThreadIds,
  setPendingComposerStream,
  handleCancelStream,
  notifyRuntimeTransient,
  refreshThreadContext,
}: UseTurnStreamRecoveryParams): UseTurnStreamRecoveryResult {
  const detachReasonRef = useRef<StreamDetachReason | null>(null);
  const recoveringRef = useRef(false);
  const offlineWarnTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const offlineInterruptTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearOfflineTimers = useCallback(() => {
    if (offlineWarnTimerRef.current != null) {
      clearTimeout(offlineWarnTimerRef.current);
      offlineWarnTimerRef.current = null;
    }
    if (offlineInterruptTimerRef.current != null) {
      clearTimeout(offlineInterruptTimerRef.current);
      offlineInterruptTimerRef.current = null;
    }
  }, []);

  const showDetachedToast = useCallback(
    (reason: StreamDetachReason) => {
      const message =
        reason === 'sidecar_restart'
          ? t('banner.turnDetachedSidecar')
          : t('banner.turnDetachedOffline');
      toast.warning(message, {
        tag: TURN_DETACHED_TAG,
        duration: 0,
        action: {
          label: t('composer.stop'),
          onClick: () => handleCancelStream(),
        },
      });
    },
    [handleCancelStream, t],
  );

  const rebindRecoveryAssistant = useCallback(
    (banner?: string): string | undefined => {
      let reboundId: string | undefined;
      setMessages((prev) => {
        const lastId = lastAssistantMessageId(prev);
        if (!lastId) return prev;
        reboundId = lastId;
        return rebindStreamingAssistant(prev, lastId, banner) as TurnChatMessage[];
      });
      const ctx = streamRecoveryContextRef.current;
      if (ctx && reboundId) {
        ctx.assistantId = reboundId;
      }
      return reboundId;
    },
    [setMessages, streamRecoveryContextRef],
  );

  const resolveEventDeliver = useCallback((): ((
    ev: SseTurnEvent,
    filter?: { turnId: string },
  ) => void) | null => {
    return (
      streamRecoveryContextRef.current?.deliverSseEvent ??
      liveStreamDeliverRef.current ??
      null
    );
  }, [liveStreamDeliverRef, streamRecoveryContextRef]);

  const runTurnEventPoll = useCallback(
    async (threadId: string, turnId: string): Promise<boolean> => {
      const deliver = resolveEventDeliver();
      if (!deliver) {
        return false;
      }

      // Multi-session: only bind global recovery refs for the active view thread.
      if (threadId !== resumedThreadIdRef.current) {
        return false;
      }

      rebindRecoveryAssistant();

      const detail = await getThreadDetail(threadId);
      if (!(await threadTurnStillActive(threadId, turnId))) {
        return false;
      }

      const controller = new AbortController();
      streamControllersRef.current.set(threadId, controller);
      threadTurnRef.current = { threadId, turnId };
      setStreamingThreadIds((prev) => new Set(prev).add(threadId));
      setPendingComposerStream(true);

      await pollThreadTurnEvents(
        threadId,
        detail.latest_seq ?? 0,
        (ev) => deliver(ev, { turnId }),
        { signal: controller.signal, turnId },
      );

      return await threadTurnStillActive(threadId, turnId);
    },
    [
      rebindRecoveryAssistant,
      resolveEventDeliver,
      setPendingComposerStream,
      setStreamingThreadIds,
      streamControllersRef,
      threadTurnRef,
    ],
  );

  const detachActiveStream = useCallback(
    (reason: StreamDetachReason) => {
      const ctx = streamRecoveryContextRef.current;
      const { threadId, turnId } = threadTurnRef.current;
      if (!ctx?.turnId && !turnId) {
        return;
      }
      if (detachReasonRef.current != null) {
        return;
      }
      detachReasonRef.current = reason;

      const banner =
        reason === 'sidecar_restart'
          ? t('composer.runtimeSidecarRestartReconnecting')
          : t('composer.runtimeOfflineReconnecting');

      for (const [threadKey, c] of streamControllersRef.current.entries()) {
        disconnectThreadEventStream(c, threadKey);
        c.abort();
      }
      streamControllersRef.current.clear();
      // Sidecar restart / offline invalidates every in-flight SSE for this window.
      // Call without a thread_id to cancel all remaining consumers (P0.1: the
      // per-thread disconnect above may have already cleared most, but any
      // consumer armed without a controllers entry still needs to be torn down).
      void import('@tauri-apps/api/core')
        .then(({ invoke }) => invoke('runtime_cancel_sse'))
        .catch(() => {});

      const activeThreadId = ctx?.threadId || threadId || resumedThreadIdRef.current;
      if (activeThreadId) {
        setStreamingThreadIds((prev) => new Set(prev).add(activeThreadId));
      }
      setPendingComposerStream(true);

      setMessages((prev) => {
        const lastId = lastAssistantMessageId(prev);
        const targetId = lastId ?? ctx?.assistantId;
        if (!targetId) return prev;
        if (ctx) ctx.assistantId = targetId;
        return prev.map((m) => {
          if (m.id !== targetId) {
            return m.role === 'assistant' && m.isStreaming ? { ...m, isStreaming: false } : m;
          }
          const tools = (m.tools ?? []).map((tool) =>
            tool.status === 'running' ? { ...tool, status: 'error' as const } : tool,
          );
          return {
            ...m,
            tools,
            content: appendDetachBanner(m.content, banner),
            isStreaming: true,
          };
        });
      });

      showDetachedToast(reason);
      if (reason === 'runtime_offline') {
        notifyRuntimeTransient(t('banner.runtimeUnreachable', { url: getRuntimeBase() }));
      }
    },
    [
      notifyRuntimeTransient,
      resumedThreadIdRef,
      setMessages,
      setPendingComposerStream,
      setStreamingThreadIds,
      showDetachedToast,
      streamControllersRef,
      streamRecoveryContextRef,
      t,
      threadTurnRef,
    ],
  );

  const shouldSkipFinishOnAbort = useCallback(() => {
    if (detachReasonRef.current == null) {
      return false;
    }
    return true;
  }, []);

  const resumeLiveTurnStream = useCallback(async () => {
    if (recoveringRef.current || detachReasonRef.current) {
      return;
    }
    const threadId = resumedThreadIdRef.current || threadTurnRef.current.threadId;
    if (!threadId || streamingRef.current) {
      return;
    }
    if (!(await threadTurnStillActive(threadId, threadTurnRef.current.turnId || undefined))) {
      return;
    }
    if (!resolveEventDeliver()) {
      return;
    }

    let turnId = threadTurnRef.current.turnId;
    if (!turnId) {
      try {
        const detail = await getThreadDetail(threadId);
        turnId = detail.thread.latest_turn_id ?? '';
        if (turnId) {
          threadTurnRef.current = { threadId, turnId };
        }
      } catch {
        return;
      }
    }
    if (!turnId) return;

    recoveringRef.current = true;
    try {
      const stillActive = await runTurnEventPoll(threadId, turnId);
      if (!stillActive) {
        streamRecoveryContextRef.current?.finishOnce({ terminal: true });
        streamSessionRef.current?.finishOnce({ terminal: true });
      }
    } catch {
      /* best-effort */
    } finally {
      recoveringRef.current = false;
    }
  }, [
    resolveEventDeliver,
    resumedThreadIdRef,
    runTurnEventPoll,
    streamRecoveryContextRef,
    streamSessionRef,
    streamingRef,
    threadTurnRef,
  ]);

  const clearStaleStreamingUi = useCallback(
    (threadId: string) => {
      const ctx = streamRecoveryContextRef.current;
      if (ctx?.finishOnce) {
        ctx.finishOnce({ terminal: true });
      } else {
        streamSessionRef.current?.finishOnce({ terminal: true });
      }
      setMessages((prev) => {
        if (!anyAssistantStreaming(prev)) return prev;
        return clearStreamingAssistants(prev) as TurnChatMessage[];
      });
      setPendingComposerStream(false);
      setStreamingThreadIds((prev) => {
        if (!prev.has(threadId)) return prev;
        const next = new Set(prev);
        next.delete(threadId);
        return next;
      });
    },
    [
      setMessages,
      setPendingComposerStream,
      setStreamingThreadIds,
      streamRecoveryContextRef,
      streamSessionRef,
    ],
  );

  const reconcileChatFromThreadReplay = useCallback(async () => {
    const threadId = resumedThreadIdRef.current;
    if (!threadId || recoveringRef.current) {
      return;
    }
    if (detachReasonRef.current) {
      return;
    }
    // Skip if navigation raced ahead to a different active thread.
    if (threadTurnRef.current.threadId && threadTurnRef.current.threadId !== threadId) {
      return;
    }

    const turnId = threadTurnRef.current.turnId || undefined;
    let stillActive: boolean;
    try {
      stillActive = await threadTurnStillActive(threadId, turnId);
    } catch {
      return;
    }

    if (!stillActive) {
      if (streamingRef.current) {
        clearStaleStreamingUi(threadId);
      } else {
        setMessages((prev) => {
          if (!anyAssistantStreaming(prev)) return prev;
          return clearStreamingAssistants(prev) as TurnChatMessage[];
        });
      }
      return;
    }

    if (streamingRef.current) {
      return;
    }

    if (resolveEventDeliver()) {
      void resumeLiveTurnStream();
      return;
    }

    try {
      const rebuilt = await rebuildMessagesFromThreadEvents(threadId);
      if (!(await threadTurnStillActive(threadId))) {
        return;
      }
      const { messages, assistantId } = markLastAssistantStreaming(rebuilt);
      if (!assistantId) {
        return;
      }
      setMessages(messages as TurnChatMessage[]);
      setStreamingThreadIds((prev) => new Set(prev).add(threadId));
      setPendingComposerStream(true);
    } catch {
      /* keep last snapshot */
    }
  }, [
    clearStaleStreamingUi,
    resolveEventDeliver,
    resumedThreadIdRef,
    resumeLiveTurnStream,
    setMessages,
    setPendingComposerStream,
    setStreamingThreadIds,
    streamingRef,
    threadTurnRef,
  ]);

  const tryRecoverDetachedTurn = useCallback(async () => {
    if (!detachReasonRef.current || recoveringRef.current) {
      return;
    }
    const ctx = streamRecoveryContextRef.current;
    const threadId =
      ctx?.threadId || threadTurnRef.current.threadId || resumedThreadIdRef.current || '';
    const turnId = ctx?.turnId || threadTurnRef.current.turnId || '';
    if (!threadId || !turnId) {
      detachReasonRef.current = null;
      toast.dismissByTag(TURN_DETACHED_TAG);
      return;
    }

    recoveringRef.current = true;
    try {
      if (!(await threadTurnStillActive(threadId, turnId))) {
        detachReasonRef.current = null;
        clearOfflineTimers();
        toast.dismissByTag(TURN_DETACHED_TAG);
        ctx?.finishOnce({ terminal: true });
        return;
      }

      if (!resolveEventDeliver()) {
        await reconcileChatFromThreadReplay();
        return;
      }

      toast.dismissByTag(TURN_DETACHED_TAG);
      toast.info(t('composer.turnReconnecting'));

      const stillActive = await runTurnEventPoll(threadId, turnId);

      detachReasonRef.current = null;
      clearOfflineTimers();
      if (!stillActive) {
        ctx?.finishOnce({ terminal: true });
      }
      void refreshThreadContext(threadId);
    } catch (e) {
      if ((e as Error).name !== 'AbortError') {
        notifyRuntimeTransient(t('banner.turnReconnectFailed', { message: (e as Error).message }));
      }
    } finally {
      recoveringRef.current = false;
    }
  }, [
    clearOfflineTimers,
    notifyRuntimeTransient,
    refreshThreadContext,
    reconcileChatFromThreadReplay,
    resolveEventDeliver,
    resumedThreadIdRef,
    runTurnEventPoll,
    streamRecoveryContextRef,
    t,
    threadTurnRef,
  ]);

  const detachActiveStreamRef = useRef(detachActiveStream);
  detachActiveStreamRef.current = detachActiveStream;
  const tryRecoverDetachedTurnRef = useRef(tryRecoverDetachedTurn);
  tryRecoverDetachedTurnRef.current = tryRecoverDetachedTurn;
  const reconcileChatFromThreadReplayRef = useRef(reconcileChatFromThreadReplay);
  reconcileChatFromThreadReplayRef.current = reconcileChatFromThreadReplay;

  const scheduleOfflineBillingGuard = useCallback(() => {
    clearOfflineTimers();
    offlineWarnTimerRef.current = setTimeout(() => {
      if (detachReasonRef.current !== 'runtime_offline') return;
      toast.warning(t('banner.turnDetachedBillingWarn'), {
        tag: TURN_DETACHED_TAG,
        duration: 0,
        action: {
          label: t('composer.stop'),
          onClick: () => handleCancelStream(),
        },
      });
    }, OFFLINE_BILLING_WARN_MS);

    offlineInterruptTimerRef.current = setTimeout(() => {
      if (detachReasonRef.current !== 'runtime_offline') return;
      const { threadId, turnId } = threadTurnRef.current;
      void (async () => {
        try {
          await stopThreadTurn({ threadId, turnId });
        } catch {
          /* best-effort */
        }
        detachReasonRef.current = null;
        toast.dismissByTag(TURN_DETACHED_TAG);
        toast.error(t('banner.turnAutoStoppedOffline'));
        streamRecoveryContextRef.current?.finishOnce();
      })();
    }, OFFLINE_AUTO_INTERRUPT_MS);
  }, [
    clearOfflineTimers,
    handleCancelStream,
    streamRecoveryContextRef,
    t,
    threadTurnRef,
  ]);

  useEffect(() => {
    if (!desktopHost) return;
    const unlistenRestart = subscribeCurrentWebviewEvent('sidecar://restarting', () => {
      if (!streamingRef.current && !streamRecoveryContextRef.current) {
        return;
      }
      detachActiveStreamRef.current('sidecar_restart');
    });
    const unlistenReady = subscribeCurrentWebviewEvent('sidecar://ready', () => {
      dispatchSidecarReadyForPanels();
      void tryRecoverDetachedTurnRef.current();
    });
    return () => {
      unlistenRestart();
      unlistenReady();
    };
  }, [desktopHost]);

  useEffect(() => {
    if (runtimeConn === 'offline' && streamingRef.current && threadTurnRef.current.turnId) {
      if (detachReasonRef.current == null) {
        detachActiveStream('runtime_offline');
        scheduleOfflineBillingGuard();
      }
      return;
    }
    if (runtimeConn === 'connected' && detachReasonRef.current != null) {
      clearOfflineTimers();
      toast.dismissByTag(RUNTIME_TRANSIENT_TAG);
      void tryRecoverDetachedTurn();
    }
  }, [
    clearOfflineTimers,
    detachActiveStream,
    runtimeConn,
    scheduleOfflineBillingGuard,
    streamingRef,
    threadTurnRef,
    tryRecoverDetachedTurn,
  ]);

  useEffect(() => {
    if (!desktopHost) return;
    const id = setInterval(() => {
      void reconcileChatFromThreadReplayRef.current();
    }, ACTIVE_TURN_CHAT_RECONCILE_MS);
    return () => clearInterval(id);
  }, [desktopHost]);

  useEffect(() => () => clearOfflineTimers(), [clearOfflineTimers]);

  const clearDetachedState = useCallback(() => {
    detachReasonRef.current = null;
    clearOfflineTimers();
    toast.dismissByTag(TURN_DETACHED_TAG);
  }, [clearOfflineTimers]);

  return {
    detachReasonRef,
    shouldSkipFinishOnAbort,
    detachActiveStream,
    tryRecoverDetachedTurn,
    clearDetachedState,
  };
}
