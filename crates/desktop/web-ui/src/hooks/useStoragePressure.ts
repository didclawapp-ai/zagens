import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type MutableRefObject,
} from 'react';
import {
  fetchStoragePressure,
  type StoragePressureSnapshot,
  worstStorageLevel,
} from '../lib/storagePressure';
import { resolveActiveThreadTurn } from '../lib/chat/streamContextAccess';
import type { StreamContextRegistry } from '../hooks/useStreamContextRegistry';
import { stopThreadTurn } from '../api/turnControl';
import { toast } from '../lib/toast';

const POLL_MS = 20_000;

export type UseStoragePressureParams = {
  desktopHost: boolean;
  workspaceRoot: string;
  streaming: boolean;
  streamRegistry: StreamContextRegistry;
  resumedThreadId: string | null;
  handleCancelStream: () => void;
  t: (key: string, params?: Record<string, string>) => string;
};

export type UseStoragePressureResult = {
  snapshot: StoragePressureSnapshot | null;
  pauseTurns: boolean;
  level: 'ok' | 'warn' | 'critical';
  refresh: () => void;
};

export function useStoragePressure({
  desktopHost,
  workspaceRoot,
  streaming,
  streamRegistry,
  resumedThreadId,
  handleCancelStream,
  t,
}: UseStoragePressureParams): UseStoragePressureResult {
  const [snapshot, setSnapshot] = useState<StoragePressureSnapshot | null>(null);
  const autoPausedRef = useRef(false);
  const criticalToastShownRef = useRef(false);

  const pauseTurns = snapshot?.pause_turns ?? false;
  const level = worstStorageLevel(snapshot);

  const runPauseForCritical = useCallback(async () => {
    const { threadId, turnId } = resolveActiveThreadTurn(streamRegistry, resumedThreadId);
    handleCancelStream();
    if (threadId.trim() && turnId.trim()) {
      try {
        await stopThreadTurn({ threadId, turnId });
      } catch {
        /* stream teardown already requested */
      }
    }
  }, [handleCancelStream, resumedThreadId, streamRegistry]);

  const refresh = useCallback(() => {
    if (!desktopHost) {
      setSnapshot(null);
      return;
    }
    void fetchStoragePressure(workspaceRoot)
      .then((next) => {
        if (!next) return;
        setSnapshot(next);
        if (next.pause_turns) {
          if (streaming && !autoPausedRef.current) {
            autoPausedRef.current = true;
            void runPauseForCritical();
          }
          if (!criticalToastShownRef.current) {
            criticalToastShownRef.current = true;
            toast.error(t('storage.criticalPaused'), { duration: 0 });
          }
        } else {
          autoPausedRef.current = false;
          criticalToastShownRef.current = false;
        }
      })
      .catch(() => {});
  }, [desktopHost, workspaceRoot, streaming, runPauseForCritical, t]);

  // `refresh` closes over `runPauseForCritical`, whose identity churns every
  // render because `streamRegistry` (passed from App) is a fresh object each
  // render. Putting `refresh` in the effect's deps would re-run the effect on
  // every render, calling `refresh()` synchronously → `setSnapshot` → re-render
  // → effect re-run → ... an unbounded `get_storage_pressure` fetch loop.
  // Hold the latest `refresh` in a ref and gate the effect on `desktopHost` only.
  const refreshRef = useRef(refresh);
  refreshRef.current = refresh;

  useEffect(() => {
    if (!desktopHost) return;
    const tick = () => refreshRef.current();
    tick();
    const id = window.setInterval(tick, POLL_MS);
    return () => window.clearInterval(id);
  }, [desktopHost]);

  useEffect(() => {
    if (!desktopHost || !pauseTurns || !streaming) return;
    if (autoPausedRef.current) return;
    autoPausedRef.current = true;
    void runPauseForCritical();
  }, [desktopHost, pauseTurns, streaming, runPauseForCritical]);

  return { snapshot, pauseTurns, level, refresh };
}
