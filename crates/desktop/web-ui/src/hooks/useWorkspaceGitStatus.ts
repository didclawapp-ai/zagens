import { useEffect, useRef, useState } from 'react';
import { getWorkspaceStatus, type WorkspaceStatusResponse } from '../api/client';

/** Idle poll after first probe; skip entirely for non-git workspaces. */
const POLL_MS = 45_000;
const FIRST_PROBE_DELAY_MS = 12_000;
const VISIBILITY_DEBOUNCE_MS = 1_200;

export type WorkspaceGitBadgeInfo = {
  branch: string;
  dirty: number;
};

/**
 * Lazy workspace git status for the Diff inspector tab badge.
 * Kept out of Composer — git belongs with the Diff surface.
 */
export function useWorkspaceGitStatus(
  workspaceRoot: string,
  streaming = false,
): WorkspaceGitBadgeInfo | null {
  const [status, setStatus] = useState<WorkspaceStatusResponse | null>(null);
  const wasStreamingRef = useRef(streaming);
  const tickInFlightRef = useRef(false);

  useEffect(() => {
    const root = workspaceRoot.trim();
    if (!root) {
      setStatus(null);
      return;
    }
    let cancelled = false;
    let intervalId: number | undefined;
    let visTimer: number | undefined;

    const tick = async () => {
      if (tickInFlightRef.current) return;
      tickInFlightRef.current = true;
      try {
        const s = await getWorkspaceStatus(root);
        if (cancelled) return;
        setStatus(s);
        if (!s.git_repo && intervalId != null) {
          window.clearInterval(intervalId);
          intervalId = undefined;
        }
      } catch {
        if (!cancelled) setStatus(null);
      } finally {
        tickInFlightRef.current = false;
      }
    };

    const scheduleIdleFirst = () => {
      const start = () => {
        if (cancelled) return;
        void tick();
        intervalId = window.setInterval(() => {
          if (document.visibilityState === 'visible') void tick();
        }, POLL_MS);
      };
      if (typeof window.requestIdleCallback === 'function') {
        const idleId = window.requestIdleCallback(start, { timeout: FIRST_PROBE_DELAY_MS });
        return () => window.cancelIdleCallback(idleId);
      }
      const delayId = window.setTimeout(start, FIRST_PROBE_DELAY_MS);
      return () => window.clearTimeout(delayId);
    };

    const cancelFirst = scheduleIdleFirst();

    const onVis = () => {
      if (document.visibilityState !== 'visible') return;
      if (visTimer != null) window.clearTimeout(visTimer);
      visTimer = window.setTimeout(() => {
        if (!cancelled) void tick();
      }, VISIBILITY_DEBOUNCE_MS);
    };
    document.addEventListener('visibilitychange', onVis);

    return () => {
      cancelled = true;
      cancelFirst();
      if (intervalId != null) window.clearInterval(intervalId);
      if (visTimer != null) window.clearTimeout(visTimer);
      document.removeEventListener('visibilitychange', onVis);
    };
  }, [workspaceRoot]);

  useEffect(() => {
    if (wasStreamingRef.current && !streaming && workspaceRoot.trim()) {
      void getWorkspaceStatus(workspaceRoot.trim())
        .then(setStatus)
        .catch(() => setStatus(null));
    }
    wasStreamingRef.current = streaming;
  }, [streaming, workspaceRoot]);

  if (!status?.git_repo) return null;
  return {
    branch: status.branch?.trim() || 'HEAD',
    dirty: status.staged + status.unstaged + status.untracked,
  };
}
