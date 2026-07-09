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
import { mergeThreadTranscript } from './turnSend/completeStreamUi';
import {
  collectReconcileThreadIds,
} from '../lib/chat/streamContextStore';
import {
  applyThreadStatusEvent,
  getThreadStatusEntry,
  isThreadStreamActive,
} from '../lib/chat/threadStatusStore';
import {
  hasAnyActiveStreamHandle,
  invokeFinishOnce,
  patchRecoveryAssistantId,
  readRecoveryCtx,
  readThreadTurn,
  resolveActiveThreadTurn,
  resolveEventDeliver,
  writeThreadTurn,
} from '../lib/chat/streamContextAccess';
import type { StreamContextRegistry } from './useStreamContextRegistry';
import type { FinishOnceOptions } from './useTurnStream';
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
  streamControllersRef: MutableRefObject<Map<string, AbortController>>;
  setMessages: Dispatch<SetStateAction<TurnChatMessage[]>>;
  setPendingComposerStream: Dispatch<SetStateAction<boolean>>;
  handleCancelStream: () => void;
  notifyRuntimeTransient: (message: string) => void;
  refreshThreadContext: (threadId: string) => Promise<void>;
  streamRegistry: StreamContextRegistry;
  /** When true, reconcile must not re-lock Composer after user Stop (agent_wait lag). */
  userStopRequestedRef?: MutableRefObject<boolean>;
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

/** True when local authority already says idle (user Stop / SSE idle) — do not re-lock. */
function threadStoreSaysIdle(threadId: string): boolean {
  const entry = getThreadStatusEntry(threadId);
  if (!entry) return false;
  return !isThreadStreamActive(entry.status);
}

export function useTurnStreamRecovery({
  t,
  desktopHost,
  runtimeConn,
  streamingRef,
  resumedThreadIdRef,
  streamControllersRef,
  setMessages,
  setPendingComposerStream,
  handleCancelStream,
  notifyRuntimeTransient,
  refreshThreadContext,
  streamRegistry,
  userStopRequestedRef,
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
      const activeThreadId = resumedThreadIdRef.current;
      let reboundId: string | undefined;
      setMessages((prev) => {
        const lastId = lastAssistantMessageId(prev);
        if (!lastId) return prev;
        reboundId = lastId;
        return rebindStreamingAssistant(prev, lastId, banner) as TurnChatMessage[];
      });
      if (activeThreadId && reboundId) {
        patchRecoveryAssistantId(streamRegistry, activeThreadId, reboundId);
      }
      return reboundId;
    },
    [resumedThreadIdRef, setMessages, streamRegistry],
  );

  const resolveEventDeliverForActive = useCallback(
    (): ((ev: SseTurnEvent, filter?: { turnId: string }) => void) | null =>
      resolveEventDeliver(streamRegistry, resumedThreadIdRef.current),
    [resumedThreadIdRef, streamRegistry],
  );

  const runTurnEventPoll = useCallback(
    async (threadId: string, turnId: string): Promise<boolean> => {
      if (userStopRequestedRef?.current || threadStoreSaysIdle(threadId)) {
        return false;
      }
      const deliver = resolveEventDeliverForActive();
      if (!deliver) {
        return false;
      }

      if (threadId !== resumedThreadIdRef.current) {
        return false;
      }

      rebindRecoveryAssistant();

      const detail = await getThreadDetail(threadId);
      if (!(await threadTurnStillActive(threadId, turnId))) {
        return false;
      }
      if (userStopRequestedRef?.current || threadStoreSaysIdle(threadId)) {
        return false;
      }

      const controller = new AbortController();
      streamControllersRef.current.set(threadId, controller);
      writeThreadTurn(streamRegistry, threadId, turnId);
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
      resolveEventDeliverForActive,
      resumedThreadIdRef,
      setPendingComposerStream,
      streamControllersRef,
      streamRegistry,
      userStopRequestedRef,
    ],
  );

  const detachActiveStream = useCallback(
    (reason: StreamDetachReason) => {
      const activeThreadId = resumedThreadIdRef.current ?? '';
      const recovery = readRecoveryCtx(streamRegistry, activeThreadId);
      const activeTurn = readThreadTurn(streamRegistry, activeThreadId);
      if (!recovery?.turnId && !activeTurn.turnId) {
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
      void import('@tauri-apps/api/core')
        .then(({ invoke }) => invoke('runtime_cancel_sse'))
        .catch(() => {});

      const threadId = recovery?.threadId || activeTurn.threadId || activeThreadId;
      if (threadId) {
        setPendingComposerStream(true);
      }

      setMessages((prev) => {
        const lastId = lastAssistantMessageId(prev);
        const targetId = lastId ?? recovery?.assistantId;
        if (!targetId) return prev;
        if (threadId) {
          patchRecoveryAssistantId(streamRegistry, threadId, targetId);
        }
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
      showDetachedToast,
      streamControllersRef,
      streamRegistry,
      t,
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
    if (userStopRequestedRef?.current) {
      return;
    }
    const activeTurn = resolveActiveThreadTurn(streamRegistry, resumedThreadIdRef.current);
    const threadId = resumedThreadIdRef.current || activeTurn.threadId;
    if (!threadId || streamingRef.current) {
      return;
    }
    if (threadStoreSaysIdle(threadId)) {
      return;
    }
    if (!(await threadTurnStillActive(threadId, activeTurn.turnId || undefined))) {
      return;
    }
    if (!resolveEventDeliverForActive()) {
      return;
    }

    let turnId = activeTurn.turnId;
    if (!turnId) {
      try {
        const detail = await getThreadDetail(threadId);
        turnId = detail.thread.latest_turn_id ?? '';
        if (turnId) {
          writeThreadTurn(streamRegistry, threadId, turnId);
        }
      } catch {
        return;
      }
    }
    if (!turnId) return;
    if (userStopRequestedRef?.current || threadStoreSaysIdle(threadId)) {
      return;
    }

    recoveringRef.current = true;
    try {
      const stillActive = await runTurnEventPoll(threadId, turnId);
      if (!stillActive) {
        invokeFinishOnce(streamRegistry, threadId, { terminal: true });
      }
    } catch {
      /* best-effort */
    } finally {
      recoveringRef.current = false;
    }
  }, [
    resolveEventDeliverForActive,
    resumedThreadIdRef,
    runTurnEventPoll,
    streamRegistry,
    streamingRef,
    userStopRequestedRef,
  ]);

  const clearStaleStreamingUi = useCallback(
    (threadId: string) => {
      invokeFinishOnce(streamRegistry, threadId, { terminal: true });
      streamRegistry.patchContext(threadId, {
        isStreaming: false,
        pendingApproval: null,
      });
      // Self-heal the authoritative store when the probe detects the backend
      // turn is no longer active but a `thread.status: idle` event was missed
      // (disconnect / sidecar restart). Without this the spinner + composer
      // lock — both derived from the store — would ghost permanently.
      applyThreadStatusEvent({ threadId, status: 'idle', source: 'reconcile' });
      setMessages((prev) => {
        if (!anyAssistantStreaming(prev)) return prev;
        return clearStreamingAssistants(prev) as TurnChatMessage[];
      });
      setPendingComposerStream(false);
    },
    [
      setMessages,
      setPendingComposerStream,
      streamRegistry,
    ],
  );

  const clearBackgroundStreamingUi = useCallback(
    (threadId: string) => {
      streamRegistry.patchContext(threadId, {
        isStreaming: false,
        pendingApproval: null,
      });
      // Self-heal the authoritative store (see `clearStaleStreamingUi`).
      applyThreadStatusEvent({ threadId, status: 'idle', source: 'reconcile' });
    },
    [streamRegistry],
  );

  const resolveTurnIdForThread = useCallback(
    (threadId: string, _isActiveView: boolean): string | undefined =>
      readThreadTurn(streamRegistry, threadId).turnId || undefined,
    [streamRegistry],
  );

  const reconcileSingleThread = useCallback(
    async (threadId: string) => {
      const tid = threadId.trim();
      if (!tid) return;

      const isActiveView = tid === resumedThreadIdRef.current;
      const turnId = resolveTurnIdForThread(tid, isActiveView);
      let stillActive: boolean;
      try {
        stillActive = await threadTurnStillActive(tid, turnId);
      } catch {
        return;
      }

      if (!stillActive) {
        streamControllersRef.current.delete(tid);
        if (isActiveView) {
          if (streamingRef.current) {
            clearStaleStreamingUi(tid);
          } else {
            setMessages((prev) => {
              if (!anyAssistantStreaming(prev)) return prev;
              return clearStreamingAssistants(prev) as TurnChatMessage[];
            });
            clearBackgroundStreamingUi(tid);
          }
        } else {
          clearBackgroundStreamingUi(tid);
        }
        return;
      }

      if (streamControllersRef.current.has(tid)) {
        return;
      }

      // User Stop (or store already idle) while turn DB may still be in_progress
      // during agent_wait — never re-lock Composer / resume the live stream.
      if (userStopRequestedRef?.current || threadStoreSaysIdle(tid)) {
        return;
      }

      if (isActiveView) {
        if (streamingRef.current) {
          return;
        }
        if (resolveEventDeliverForActive()) {
          void resumeLiveTurnStream();
          return;
        }
        try {
          const rebuilt = await rebuildMessagesFromThreadEvents(tid);
          if (!(await threadTurnStillActive(tid, turnId))) {
            return;
          }
          if (userStopRequestedRef?.current || threadStoreSaysIdle(tid)) {
            return;
          }
          const live = (streamRegistry?.getContext(tid)?.messages ?? []) as TurnChatMessage[];
          const merged = mergeThreadTranscript(live, rebuilt as TurnChatMessage[]);
          const { messages, assistantId } = markLastAssistantStreaming(merged);
          if (!assistantId) {
            return;
          }
          setMessages(messages as TurnChatMessage[]);
          setPendingComposerStream(true);
        } catch {
          /* keep last snapshot */
        }
        return;
      }

      // Background thread: refresh registry transcript for reattach.
      try {
        const rebuilt = await rebuildMessagesFromThreadEvents(tid);
        if (!(await threadTurnStillActive(tid, turnId))) {
          clearBackgroundStreamingUi(tid);
          return;
        }
        const live = (streamRegistry?.getContext(tid)?.messages ?? []) as TurnChatMessage[];
        const merged = mergeThreadTranscript(live, rebuilt as TurnChatMessage[]);
        const { messages: marked } = markLastAssistantStreaming(merged);
        const sessionId = streamRegistry?.getContext(tid)?.sessionId ?? null;
        streamRegistry?.ensureContext(tid, sessionId);
        streamRegistry?.patchContext(tid, {
          messages: marked as TurnChatMessage[],
          isStreaming: true,
        });
      } catch {
        /* keep last snapshot */
      }
    },
    [
      clearBackgroundStreamingUi,
      clearStaleStreamingUi,
      resolveEventDeliverForActive,
      resolveTurnIdForThread,
      resumeLiveTurnStream,
      setMessages,
      setPendingComposerStream,
      streamControllersRef,
      streamRegistry,
      streamingRef,
      userStopRequestedRef,
    ],
  );

  const reconcileChatFromThreadReplay = useCallback(async () => {
    if (recoveringRef.current) {
      return;
    }
    if (detachReasonRef.current) {
      return;
    }

    const threadIds = collectReconcileThreadIds(resumedThreadIdRef.current);
    if (threadIds.length === 0) {
      return;
    }

    for (const threadId of threadIds) {
      await reconcileSingleThread(threadId);
    }
  }, [
    reconcileSingleThread,
    resumedThreadIdRef,
  ]);

  const tryRecoverDetachedTurn = useCallback(async () => {
    if (!detachReasonRef.current || recoveringRef.current) {
      return;
    }
    const activeThreadId = resumedThreadIdRef.current ?? '';
    const recovery = readRecoveryCtx(streamRegistry, activeThreadId);
    const activeTurn = readThreadTurn(streamRegistry, activeThreadId);
    const threadId = recovery?.threadId || activeTurn.threadId || activeThreadId;
    const turnId = recovery?.turnId || activeTurn.turnId || '';
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
        invokeFinishOnce(streamRegistry, threadId, { terminal: true });
        return;
      }

      if (!resolveEventDeliverForActive()) {
        await reconcileChatFromThreadReplay();
        return;
      }

      toast.dismissByTag(TURN_DETACHED_TAG);
      toast.info(t('composer.turnReconnecting'));

      const stillActive = await runTurnEventPoll(threadId, turnId);

      detachReasonRef.current = null;
      clearOfflineTimers();
      if (!stillActive) {
        invokeFinishOnce(streamRegistry, threadId, { terminal: true });
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
    resolveEventDeliverForActive,
    resumedThreadIdRef,
    runTurnEventPoll,
    streamRegistry,
    t,
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
      const activeTurn = resolveActiveThreadTurn(streamRegistry, resumedThreadIdRef.current);
      const { threadId, turnId } = activeTurn;
      void (async () => {
        try {
          await stopThreadTurn({ threadId, turnId });
        } catch {
          /* best-effort */
        }
        detachReasonRef.current = null;
        toast.dismissByTag(TURN_DETACHED_TAG);
        toast.error(t('banner.turnAutoStoppedOffline'));
        invokeFinishOnce(streamRegistry, threadId);
      })();
    }, OFFLINE_AUTO_INTERRUPT_MS);
  }, [
    clearOfflineTimers,
    handleCancelStream,
    resumedThreadIdRef,
    streamRegistry,
    t,
  ]);

  useEffect(() => {
    if (!desktopHost) return;
    const unlistenRestart = subscribeCurrentWebviewEvent('sidecar://restarting', () => {
      if (
        !streamingRef.current &&
        !hasAnyActiveStreamHandle(streamRegistry, resumedThreadIdRef.current)
      ) {
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
    const activeTurn = resolveActiveThreadTurn(streamRegistry, resumedThreadIdRef.current);
    if (runtimeConn === 'offline' && streamingRef.current && activeTurn.turnId) {
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
    streamRegistry,
    streamingRef,
    tryRecoverDetachedTurn,
  ]);

  useEffect(() => {
    if (!desktopHost) return;
    // Compensation sync when local SSE state drifts from server (S2.1 fallback).
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
