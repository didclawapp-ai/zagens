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
import type { StreamSessionControl } from './useTurnStream';
import type { TurnChatMessage } from './useTurnSend';

/** Why the live SSE consumer was detached (backend turn may still run). */
export type StreamDetachReason = 'sidecar_restart' | 'runtime_offline';

export const TURN_DETACHED_TAG = 'turn-detached';

/** After this long offline while detached, auto-interrupt the runtime turn (API billing guard). */
export const OFFLINE_AUTO_INTERRUPT_MS = 120_000;

/** Warn the user that billing may continue before auto-interrupt. */
export const OFFLINE_BILLING_WARN_MS = 15_000;

export type StreamRecoveryContext = {
  assistantId: string;
  threadId: string;
  turnId: string;
  deliverSseEvent: (ev: SseTurnEvent, filter?: { turnId: string }) => void;
  finishOnce: () => void;
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

      for (const c of streamControllersRef.current.values()) {
        disconnectThreadEventStream(c);
        c.abort();
      }
      streamControllersRef.current.clear();
      void import('@tauri-apps/api/core')
        .then(({ invoke }) => invoke('runtime_cancel_sse'))
        .catch(() => {});

      const activeThreadId = ctx?.threadId || threadId || resumedThreadIdRef.current;
      if (activeThreadId) {
        setStreamingThreadIds((prev) => new Set(prev).add(activeThreadId));
      }
      setPendingComposerStream(false);

      const assistantId = ctx?.assistantId;
      if (assistantId) {
        setMessages((prev) =>
          prev.map((m) => {
            if (m.id !== assistantId) return m;
            const tools = (m.tools ?? []).map((tool) =>
              tool.status === 'running' ? { ...tool, status: 'error' as const } : tool,
            );
            return {
              ...m,
              tools,
              content: appendDetachBanner(m.content, banner),
              isStreaming: true,
            };
          }),
        );
      }

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

  const tryRecoverDetachedTurn = useCallback(async () => {
    if (!detachReasonRef.current || recoveringRef.current) {
      return;
    }
    const ctx = streamRecoveryContextRef.current;
    const threadId =
      ctx?.threadId || threadTurnRef.current.threadId || resumedThreadIdRef.current || '';
    const turnId = ctx?.turnId || threadTurnRef.current.turnId || '';
    if (!threadId || !turnId || !ctx) {
      detachReasonRef.current = null;
      toast.dismissByTag(TURN_DETACHED_TAG);
      return;
    }

    recoveringRef.current = true;
    try {
      const detail = await getThreadDetail(threadId);
      if (!(await threadTurnStillActive(threadId, turnId))) {
        detachReasonRef.current = null;
        clearOfflineTimers();
        toast.dismissByTag(TURN_DETACHED_TAG);
        ctx.finishOnce();
        return;
      }

      toast.dismissByTag(TURN_DETACHED_TAG);
      toast.info(t('composer.turnReconnecting'));

      const controller = new AbortController();
      streamControllersRef.current.set(threadId, controller);
      threadTurnRef.current = { threadId, turnId };
      setStreamingThreadIds((prev) => new Set(prev).add(threadId));

      await pollThreadTurnEvents(
        threadId,
        detail.latest_seq ?? 0,
        (ev) => ctx.deliverSseEvent(ev, { turnId }),
        { signal: controller.signal, turnId },
      );

      detachReasonRef.current = null;
      clearOfflineTimers();
      ctx.finishOnce();
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
    resumedThreadIdRef,
    setStreamingThreadIds,
    streamControllersRef,
    streamRecoveryContextRef,
    t,
    threadTurnRef,
  ]);

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
      detachActiveStream('sidecar_restart');
    });
    const unlistenReady = subscribeCurrentWebviewEvent('sidecar://ready', () => {
      dispatchSidecarReadyForPanels();
      void tryRecoverDetachedTurn();
    });
    return () => {
      unlistenRestart();
      unlistenReady();
    };
  }, [
    desktopHost,
    detachActiveStream,
    streamRecoveryContextRef,
    streamingRef,
    tryRecoverDetachedTurn,
  ]);

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
