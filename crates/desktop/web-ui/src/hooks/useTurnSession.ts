import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from 'react';
import {
  deleteSession,
  getSessions,
  persistThreadSession,
  type SessionInfo,
} from '../api/client';
import { confirmDialog } from '../lib/confirmDialog';
import {
  normalizeWorkspaceForApi,
  workspacesMatch,
} from '../lib/defaultWorkspace';
import { SESSION_CHECKPOINT_STREAMING_MS } from '../lib/runtimePoll';
import { toast } from '../lib/toast';
import {
  clearStoredActiveSessionId,
  loadStoredActiveSessionId,
  saveStoredActiveSessionId,
} from '../lib/windowBridge';

export type UseTurnSessionParams = {
  t: (key: string, params?: Record<string, string>) => string;
  showAllSessions: boolean;
  selectedWorkspace: string;
  streamingRef: MutableRefObject<boolean>;
  setRuntimeSessionEstablished: Dispatch<SetStateAction<boolean>>;
  reconcileRuntimeAfterFetchFailure: () => void;
  notifyRuntimeTransient: (message: string) => void;
  refreshSessionsRef: MutableRefObject<() => Promise<void>>;
  onRestoreSession: (sessionId: string) => void;
  onClearActiveSession: () => void;
};

export type UseTurnSessionResult = {
  sessions: SessionInfo[];
  setSessions: Dispatch<SetStateAction<SessionInfo[]>>;
  activeSessionId: string | null;
  setActiveSessionId: Dispatch<SetStateAction<string | null>>;
  activeSessionIdRef: MutableRefObject<string | null>;
  resumedThreadId: string | null;
  setResumedThreadId: Dispatch<SetStateAction<string | null>>;
  resumedThreadIdRef: MutableRefObject<string | null>;
  visibleSessions: SessionInfo[];
  refreshSessions: () => Promise<void>;
  handleDeleteSession: (sessionId: string) => Promise<void>;
};

export function useTurnSession({
  t,
  showAllSessions,
  selectedWorkspace,
  streamingRef,
  setRuntimeSessionEstablished,
  reconcileRuntimeAfterFetchFailure,
  notifyRuntimeTransient,
  refreshSessionsRef,
  onRestoreSession,
  onClearActiveSession,
}: UseTurnSessionParams): UseTurnSessionResult {
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [resumedThreadId, setResumedThreadId] = useState<string | null>(null);

  const activeSessionIdRef = useRef<string | null>(null);
  const resumedThreadIdRef = useRef<string | null>(null);
  const startupSessionRestoredRef = useRef(false);

  useEffect(() => {
    activeSessionIdRef.current = activeSessionId;
  }, [activeSessionId]);

  useEffect(() => {
    resumedThreadIdRef.current = resumedThreadId;
  }, [resumedThreadId]);

  const visibleSessions = (() => {
    if (showAllSessions) return sessions;
    const root = normalizeWorkspaceForApi(selectedWorkspace);
    if (!root) return sessions;
    return sessions.filter((s) => {
      if (!s.workspace?.trim()) return true;
      return workspacesMatch(s.workspace, root);
    });
  })();

  const notifyRuntimeTransientRef = useRef(notifyRuntimeTransient);
  notifyRuntimeTransientRef.current = notifyRuntimeTransient;
  const reconcileRuntimeAfterFetchFailureRef = useRef(reconcileRuntimeAfterFetchFailure);
  reconcileRuntimeAfterFetchFailureRef.current = reconcileRuntimeAfterFetchFailure;
  const setRuntimeSessionEstablishedRef = useRef(setRuntimeSessionEstablished);
  setRuntimeSessionEstablishedRef.current = setRuntimeSessionEstablished;

  const refreshSessions = useCallback(async () => {
    try {
      const list = await getSessions();
      setSessions(list);
      setRuntimeSessionEstablishedRef.current(true);
      toast.dismissAll();
    } catch (e) {
      const err = e as Error & { status?: number };
      if (err.status === 401) {
        notifyRuntimeTransientRef.current(t('banner.unauthorized'));
      } else {
        notifyRuntimeTransientRef.current(t('banner.loadSessionsError', { message: err.message }));
      }
      reconcileRuntimeAfterFetchFailureRef.current();
    }
  }, [t]);

  refreshSessionsRef.current = refreshSessions;

  useEffect(() => {
    if (!streamingRef.current || !resumedThreadId) {
      return;
    }
    const tid = resumedThreadId;
    const tick = () => {
      void (async () => {
        try {
          const res = await persistThreadSession(tid, activeSessionIdRef.current);
          setActiveSessionId(res.session_id);
          saveStoredActiveSessionId(res.session_id);
          await refreshSessions();
        } catch {
          /* avoid toast spam — turn-complete persist will retry */
        }
      })();
    };
    const id = window.setInterval(tick, SESSION_CHECKPOINT_STREAMING_MS);
    return () => window.clearInterval(id);
  }, [resumedThreadId, refreshSessions, streamingRef]);

  useEffect(() => {
    const onVis = () => {
      if (document.visibilityState !== 'hidden') {
        return;
      }
      if (!resumedThreadId) {
        return;
      }
      const tid = resumedThreadId;
      void (async () => {
        try {
          const res = await persistThreadSession(tid, activeSessionIdRef.current);
          setActiveSessionId(res.session_id);
          saveStoredActiveSessionId(res.session_id);
          await refreshSessions();
        } catch {
          /* ignore */
        }
      })();
    };
    document.addEventListener('visibilitychange', onVis);
    return () => document.removeEventListener('visibilitychange', onVis);
  }, [resumedThreadId, refreshSessions]);

  useEffect(() => {
    if (sessions.length === 0 || startupSessionRestoredRef.current) {
      return;
    }
    const stored = loadStoredActiveSessionId();
    if (!stored) {
      startupSessionRestoredRef.current = true;
      return;
    }
    if (!sessions.some((s) => s.id === stored)) {
      clearStoredActiveSessionId();
      startupSessionRestoredRef.current = true;
      return;
    }
    startupSessionRestoredRef.current = true;
    onRestoreSession(stored);
  }, [sessions, onRestoreSession]);

  const handleDeleteSession = useCallback(
    async (sessionId: string) => {
      if (!(await confirmDialog(t('sidebar.deleteConfirm')))) return;
      toast.dismissAll();
      try {
        await deleteSession(sessionId);
        if (activeSessionId === sessionId) {
          onClearActiveSession();
        }
        await refreshSessions();
      } catch (e) {
        const err = e as Error & { status?: number };
        toast.error(t('banner.deleteSessionFailed', { message: err.message }));
      }
    },
    [activeSessionId, onClearActiveSession, refreshSessions, t],
  );

  return {
    sessions,
    setSessions,
    activeSessionId,
    setActiveSessionId,
    activeSessionIdRef,
    resumedThreadId,
    setResumedThreadId,
    resumedThreadIdRef,
    visibleSessions,
    refreshSessions,
    handleDeleteSession,
  };
}
