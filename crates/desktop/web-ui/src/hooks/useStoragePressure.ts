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
import { stopThreadTurn } from '../api/turnControl';
import { toast } from '../lib/toast';

const POLL_MS = 20_000;

export type UseStoragePressureParams = {
  desktopHost: boolean;
  workspaceRoot: string;
  streaming: boolean;
  threadTurnRef: MutableRefObject<{ threadId: string; turnId: string }>;
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
  threadTurnRef,
  handleCancelStream,
  t,
}: UseStoragePressureParams): UseStoragePressureResult {
  const [snapshot, setSnapshot] = useState<StoragePressureSnapshot | null>(null);
  const autoPausedRef = useRef(false);
  const criticalToastShownRef = useRef(false);

  const pauseTurns = snapshot?.pause_turns ?? false;
  const level = worstStorageLevel(snapshot);

  const runPauseForCritical = useCallback(async () => {
    const { threadId, turnId } = threadTurnRef.current;
    handleCancelStream();
    if (threadId.trim() && turnId.trim()) {
      try {
        await stopThreadTurn({ threadId, turnId });
      } catch {
        /* stream teardown already requested */
      }
    }
  }, [handleCancelStream, threadTurnRef]);

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

  useEffect(() => {
    if (!desktopHost) return;
    refresh();
    const id = window.setInterval(refresh, POLL_MS);
    return () => window.clearInterval(id);
  }, [desktopHost, refresh]);

  useEffect(() => {
    if (!desktopHost || !pauseTurns || !streaming) return;
    if (autoPausedRef.current) return;
    autoPausedRef.current = true;
    void runPauseForCritical();
  }, [desktopHost, pauseTurns, streaming, runPauseForCritical]);

  return { snapshot, pauseTurns, level, refresh };
}
