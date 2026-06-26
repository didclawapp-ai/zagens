import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type MutableRefObject,
} from 'react';
import {
  getRuntimeBase,
  initRuntimeConfig,
  invalidateRuntimeBootReadyCache,
  probeRuntimeConnection,
  waitForRuntimeBootReady,
  waitForRuntimeReady,
  type RuntimeConnectionState,
} from '../api/client';
import {
  RUNTIME_PROBE_INTERVAL_IDLE_MS,
  RUNTIME_PROBE_INTERVAL_STREAMING_MS,
} from '../lib/runtimePoll';
import { RUNTIME_TRANSIENT_TAG, toast } from '../lib/toast';

const PROBE_FAILS_BEFORE_OFFLINE = 3;
/** When a “busy” (degraded) state is entered and this many ms elapse, do one fast extra probe. */
const BUSY_FAST_RECOVERY_DELAY_MS = 1000;

export type UseRuntimeConnectionParams = {
  streaming: boolean;
  streamingRef: MutableRefObject<boolean>;
  t: (key: string, params?: Record<string, string>) => string;
  /** Latest session refresh (App wires after `refreshSessions` is defined). */
  refreshSessionsRef: MutableRefObject<() => Promise<void>>;
};

export type UseRuntimeConnectionResult = {
  runtimeConn: RuntimeConnectionState;
  runtimeSessionEstablished: boolean;
  setRuntimeSessionEstablished: React.Dispatch<React.SetStateAction<boolean>>;
  runtimeReachability: { streaming: boolean; sessionEstablished: boolean };
  retryConnect: () => void;
  reconcileRuntimeAfterFetchFailure: () => void;
  dismissRuntimeTransient: () => void;
  notifyRuntimeTransient: (message: string) => void;
};

export function useRuntimeConnection({
  streaming,
  streamingRef,
  t,
  refreshSessionsRef,
}: UseRuntimeConnectionParams): UseRuntimeConnectionResult {
  const [runtimeConn, setRuntimeConn] = useState<RuntimeConnectionState>('checking');
  const [runtimeSessionEstablished, setRuntimeSessionEstablished] = useState(false);
  const runtimeSessionEstablishedRef = useRef(false);
  const runtimeProbeFailStreakRef = useRef(0);
  const hasConnectedOnceRef = useRef(false);
  const busyRecoveryTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearBusyRecovery = useCallback(() => {
    if (busyRecoveryTimerRef.current !== null) {
      clearTimeout(busyRecoveryTimerRef.current);
      busyRecoveryTimerRef.current = null;
    }
  }, []);

  useEffect(() => {
    runtimeSessionEstablishedRef.current = runtimeSessionEstablished;
  }, [runtimeSessionEstablished]);

  const runtimeReachability = useMemo(
    () => ({ streaming, sessionEstablished: runtimeSessionEstablished }),
    [streaming, runtimeSessionEstablished],
  );

  const dismissRuntimeTransient = useCallback(() => {
    toast.dismissByTag(RUNTIME_TRANSIENT_TAG);
  }, []);

  const retryConnectRef = useRef<() => void>(() => {});

  const notifyRuntimeTransient = useCallback(
    (message: string, options?: { onlyAfterFirstConnect?: boolean }) => {
      if (options?.onlyAfterFirstConnect && !hasConnectedOnceRef.current) {
        return;
      }
      toast.error(message, {
        tag: RUNTIME_TRANSIENT_TAG,
        duration: 0,
        action: {
          label: t('common.retryConnection'),
          onClick: () => retryConnectRef.current(),
        },
      });
    },
    [t],
  );

  const reconcileRuntimeAfterFetchFailure = useCallback(() => {
    void probeRuntimeConnection({ light: streamingRef.current }).then((s) => {
      if (s === 'connected') {
        hasConnectedOnceRef.current = true;
        runtimeProbeFailStreakRef.current = 0;
        setRuntimeSessionEstablished(true);
        setRuntimeConn('connected');
        dismissRuntimeTransient();
        return;
      }
      if (s === 'auth_mismatch') {
        runtimeProbeFailStreakRef.current = 0;
        setRuntimeConn('auth_mismatch');
        return;
      }
      if (runtimeSessionEstablishedRef.current || streamingRef.current) {
        runtimeProbeFailStreakRef.current += 1;
        if (runtimeProbeFailStreakRef.current >= PROBE_FAILS_BEFORE_OFFLINE) {
          setRuntimeConn('offline');
        }
        return;
      }
      runtimeProbeFailStreakRef.current = 0;
      setRuntimeConn('offline');
    });
  }, [dismissRuntimeTransient, streamingRef]);

  const retryConnectAndSessions = useCallback(async () => {
    toast.dismissAll();
    setRuntimeConn('checking');
    try {
      invalidateRuntimeBootReadyCache();
      await initRuntimeConfig();
      const runtimeUrl = getRuntimeBase();
      const ok = await waitForRuntimeReady({ timeoutMs: 60_000, intervalMs: 150 });
      const probed = await probeRuntimeConnection();
      if (probed === 'connected') {
        hasConnectedOnceRef.current = true;
        setRuntimeSessionEstablished(true);
        runtimeProbeFailStreakRef.current = 0;
      }
      setRuntimeConn(probed);
      if (!ok) {
        notifyRuntimeTransient(t('banner.runtimeUnreachableStartup', { url: runtimeUrl }), {
          onlyAfterFirstConnect: true,
        });
        return;
      }
      await refreshSessionsRef.current();
    } catch (e) {
      notifyRuntimeTransient(t('banner.retryFailed', { message: (e as Error).message }), {
        onlyAfterFirstConnect: true,
      });
      setRuntimeConn('offline');
    }
  }, [notifyRuntimeTransient, refreshSessionsRef, t]);

  const retryConnect = useCallback(() => {
    void retryConnectAndSessions();
  }, [retryConnectAndSessions]);

  useEffect(() => {
    retryConnectRef.current = retryConnect;
  }, [retryConnect]);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        await initRuntimeConfig();
        const ok = await waitForRuntimeBootReady({ timeoutMs: 90_000, intervalMs: 150 });
        if (!cancelled) {
          const probed = await probeRuntimeConnection();
          if (probed === 'connected') {
            hasConnectedOnceRef.current = true;
            setRuntimeSessionEstablished(true);
            runtimeProbeFailStreakRef.current = 0;
          }
          setRuntimeConn(probed);
        }
        if (cancelled) {
          return;
        }
        if (!ok) {
          notifyRuntimeTransient(t('banner.runtimeUnreachable', { url: getRuntimeBase() }), {
            onlyAfterFirstConnect: true,
          });
          return;
        }
        await refreshSessionsRef.current();
      } catch (e) {
        if (!cancelled) {
          notifyRuntimeTransient(t('banner.bootCheckFailed', { message: (e as Error).message }), {
            onlyAfterFirstConnect: true,
          });
          setRuntimeConn('offline');
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [notifyRuntimeTransient, refreshSessionsRef, t]);

  useEffect(() => {
    if (!streaming) {
      return;
    }
    let cancelled = false;
    void probeRuntimeConnection({ light: true }).then((s) => {
      if (!cancelled && s === 'connected') {
        hasConnectedOnceRef.current = true;
        runtimeProbeFailStreakRef.current = 0;
        setRuntimeSessionEstablished(true);
        setRuntimeConn('connected');
      }
    });
    return () => {
      cancelled = true;
    };
  }, [streaming]);

  useEffect(() => {
    let cancelled = false;
    const scheduleBusyRecovery = () => {
      if (runtimeSessionEstablishedRef.current) {
        clearBusyRecovery();
        busyRecoveryTimerRef.current = setTimeout(() => {
          void probeRuntimeConnection({ light: streamingRef.current }).then((s) => {
            if (s === 'connected') {
              hasConnectedOnceRef.current = true;
              runtimeProbeFailStreakRef.current = 0;
              setRuntimeSessionEstablished(true);
              setRuntimeConn('connected');
              dismissRuntimeTransient();
            }
            // If still not connected, normal poll cycle will retry later.
          });
        }, BUSY_FAST_RECOVERY_DELAY_MS);
      }
    };

    const applyProbe = (s: Exclude<RuntimeConnectionState, 'checking'>) => {
      if (s === 'connected') {
        hasConnectedOnceRef.current = true;
        runtimeProbeFailStreakRef.current = 0;
        setRuntimeSessionEstablished(true);
        setRuntimeConn('connected');
        dismissRuntimeTransient();
        clearBusyRecovery();
        return;
      }
      if (s === 'auth_mismatch') {
        runtimeProbeFailStreakRef.current = 0;
        setRuntimeConn('auth_mismatch');
        return;
      }
      if (runtimeSessionEstablishedRef.current || streamingRef.current) {
        runtimeProbeFailStreakRef.current += 1;
        if (runtimeProbeFailStreakRef.current >= PROBE_FAILS_BEFORE_OFFLINE) {
          setRuntimeConn('offline');
          scheduleBusyRecovery();
        }
        return;
      }
      runtimeProbeFailStreakRef.current = 0;
      setRuntimeConn('offline');
    };
    const tick = async () => {
      const s = await probeRuntimeConnection({ light: streamingRef.current });
      if (!cancelled) {
        applyProbe(s);
      }
    };
    void tick();
    const intervalMs = streaming
      ? RUNTIME_PROBE_INTERVAL_STREAMING_MS
      : RUNTIME_PROBE_INTERVAL_IDLE_MS;
    const id = window.setInterval(() => void tick(), intervalMs);
    return () => {
      cancelled = true;
      window.clearInterval(id);
      clearBusyRecovery();
    };
  }, [dismissRuntimeTransient, streaming, streamingRef, clearBusyRecovery]);

  return {
    runtimeConn,
    runtimeSessionEstablished,
    setRuntimeSessionEstablished,
    runtimeReachability,
    retryConnect,
    reconcileRuntimeAfterFetchFailure,
    dismissRuntimeTransient,
    notifyRuntimeTransient,
  };
}
