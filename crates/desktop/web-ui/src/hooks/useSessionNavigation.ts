import {
  useCallback,
  useRef,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from 'react';
import { getSessionDetail, getThreadDetail, resumeSessionThread } from '../api/client';
import { rebuildMessagesFromThreadEvents } from '../lib/chat/rebuildMessagesFromThread';
import {
  cacheSessionUiMessages,
  getCachedSessionUiMessages,
  type CachedUiMessage,
} from '../lib/chat/sessionUiCache';
import { mapSessionDetailToMessages } from '../lib/chat/sessionMessages';
import {
  contextWindowTokensForModel,
  type ThreadContextSnapshot,
} from '../lib/contextUsage';
import type { PreviewState } from '../components/preview/types';
import { toast } from '../lib/toast';
import { registerWindowThread } from '../lib/windowBridge';
import type { DesktopModelId, DesktopTaskTypeResolved } from '../types/desktop';
import { ACTIVE_SESSION_STORAGE_KEY } from './useTurnSession';

type NavMessage = CachedUiMessage;

export type UseSessionNavigationParams = {
  t: (key: string, params?: Record<string, string>) => string;
  selectedModel: DesktopModelId;
  activeSessionIdRef: MutableRefObject<string | null>;
  resumedThreadIdRef: MutableRefObject<string | null>;
  threadTurnRef: MutableRefObject<{ threadId: string; turnId: string }>;
  threadContextSnapshotRef: MutableRefObject<ThreadContextSnapshot | null>;
  threadContextCacheRef: MutableRefObject<Map<string, ThreadContextSnapshot>>;
  messagesRef: MutableRefObject<NavMessage[]>;
  sessionUiCacheRef: MutableRefObject<Map<string, CachedUiMessage[]>>;
  abortThreadStream: (threadId: string | null | undefined) => void;
  resetTurnPersistState: () => void;
  clearApproval: () => void;
  setMessages: Dispatch<SetStateAction<NavMessage[]>>;
  setActiveSessionId: Dispatch<SetStateAction<string | null>>;
  setResumedThreadId: Dispatch<SetStateAction<string | null>>;
  setRuntimeSessionEstablished: Dispatch<SetStateAction<boolean>>;
  setThreadTrustMode: Dispatch<SetStateAction<boolean>>;
  setPanelPreview: Dispatch<SetStateAction<PreviewState | null>>;
  setThreadDetailForContext: Dispatch<SetStateAction<import('../lib/contextUsage').ThreadDetailWithTurns | null>>;
  setLastTurnOutputTokens: Dispatch<SetStateAction<number | null>>;
  setContextWindowTokens: Dispatch<SetStateAction<number>>;
  setSelectedWorkspace: Dispatch<SetStateAction<string>>;
  setLockedThreadTaskType: Dispatch<SetStateAction<DesktopTaskTypeResolved | null>>;
  refreshThreadContext: (threadId: string) => Promise<void>;
  restoreThreadContextFromCache: (threadId: string) => void;
  reconcileRuntimeAfterFetchFailure: () => void;
  notifyRuntimeTransient: (message: string) => void;
};

export type UseSessionNavigationResult = {
  handleSelectSession: (sessionId: string) => Promise<void>;
  handleNewSession: () => void;
};

export function useSessionNavigation({
  t,
  selectedModel,
  activeSessionIdRef,
  resumedThreadIdRef,
  threadTurnRef,
  threadContextSnapshotRef,
  threadContextCacheRef,
  messagesRef,
  sessionUiCacheRef,
  abortThreadStream,
  resetTurnPersistState,
  clearApproval,
  setMessages,
  setActiveSessionId,
  setResumedThreadId,
  setRuntimeSessionEstablished,
  setThreadTrustMode,
  setPanelPreview,
  setThreadDetailForContext,
  setLastTurnOutputTokens,
  setContextWindowTokens,
  setSelectedWorkspace,
  setLockedThreadTaskType,
  refreshThreadContext,
  restoreThreadContextFromCache,
  reconcileRuntimeAfterFetchFailure,
  notifyRuntimeTransient,
}: UseSessionNavigationParams): UseSessionNavigationResult {
  const selectSessionGenerationRef = useRef(0);
  const selectSessionAbortRef = useRef<AbortController | null>(null);

  const handleSelectSession = useCallback(
    async (sessionId: string) => {
      const gen = ++selectSessionGenerationRef.current;
      selectSessionAbortRef.current?.abort();
      const selectAbort = new AbortController();
      selectSessionAbortRef.current = selectAbort;

      const outgoingSessionId = activeSessionIdRef.current;
      if (outgoingSessionId && messagesRef.current.length > 0) {
        cacheSessionUiMessages(
          sessionUiCacheRef.current,
          outgoingSessionId,
          messagesRef.current,
        );
      }
      const outgoingThreadId = resumedThreadIdRef.current;
      const outgoingSnapshot = threadContextSnapshotRef.current;
      if (outgoingThreadId && outgoingSnapshot) {
        threadContextCacheRef.current.set(outgoingThreadId, outgoingSnapshot);
      }
      if (outgoingThreadId) {
        abortThreadStream(outgoingThreadId);
      }

      toast.dismissAll();
      setActiveSessionId(sessionId);
      setResumedThreadId(null);
      setThreadTrustMode(false);
      setPanelPreview(null);
      resetTurnPersistState();

      const cachedUi = getCachedSessionUiMessages(sessionUiCacheRef.current, sessionId);
      if (cachedUi?.length) {
        setMessages(cachedUi);
        cacheSessionUiMessages(sessionUiCacheRef.current, sessionId, cachedUi);
      } else {
        setMessages([]);
      }

      try {
        const detail = await getSessionDetail(sessionId);
        if (gen !== selectSessionGenerationRef.current) {
          return;
        }
        const resumed = await resumeSessionThread(sessionId);
        if (gen !== selectSessionGenerationRef.current) {
          return;
        }
        const sessionFallback = mapSessionDetailToMessages(detail);
        if (!cachedUi?.length) {
          setMessages(sessionFallback);
        }
        resumedThreadIdRef.current = resumed.thread_id;
        setResumedThreadId(resumed.thread_id);
        setRuntimeSessionEstablished(true);
        restoreThreadContextFromCache(resumed.thread_id);
        try {
          const fromThread = await rebuildMessagesFromThreadEvents(resumed.thread_id, {
            signal: selectAbort.signal,
          });
          if (gen !== selectSessionGenerationRef.current) {
            return;
          }
          if (fromThread.length > 0) {
            setMessages(fromThread);
            cacheSessionUiMessages(sessionUiCacheRef.current, sessionId, fromThread);
          } else if (!cachedUi?.length && sessionFallback.length > 0) {
            cacheSessionUiMessages(sessionUiCacheRef.current, sessionId, sessionFallback);
          }
        } catch {
          if (!cachedUi?.length && sessionFallback.length > 0) {
            setMessages(sessionFallback);
          }
        }
        threadTurnRef.current = { threadId: resumed.thread_id, turnId: '' };
        try {
          const threadDetail = await getThreadDetail(resumed.thread_id);
          if (gen !== selectSessionGenerationRef.current) {
            return;
          }
          setThreadDetailForContext(threadDetail);
          const turns = threadDetail.turns ?? [];
          const lastTurn = turns.length > 0 ? turns[turns.length - 1] : undefined;
          const lastOut = lastTurn?.usage?.output_tokens;
          setLastTurnOutputTokens(
            lastOut != null && Number.isFinite(lastOut) && lastOut > 0 ? lastOut : null,
          );
          setContextWindowTokens(
            contextWindowTokensForModel(threadDetail.thread.model ?? selectedModel),
          );
          setSelectedWorkspace(threadDetail.thread.workspace);
          setThreadTrustMode(Boolean(threadDetail.thread.trust_mode));
          void registerWindowThread(resumed.thread_id);
          if (gen === selectSessionGenerationRef.current) {
            void refreshThreadContext(resumed.thread_id);
          }
        } catch (syncErr) {
          if (gen !== selectSessionGenerationRef.current) {
            return;
          }
          setThreadDetailForContext(null);
          setContextWindowTokens(contextWindowTokensForModel(selectedModel));
          const errMsg = syncErr instanceof Error ? syncErr.message : String(syncErr);
          notifyRuntimeTransient(t('banner.threadWorkspaceError', { errMsg }));
          reconcileRuntimeAfterFetchFailure();
        }
        try {
          localStorage.setItem(ACTIVE_SESSION_STORAGE_KEY, sessionId);
        } catch {
          /* ignore */
        }
      } catch (e) {
        if (gen !== selectSessionGenerationRef.current) {
          return;
        }
        const err = e as Error & { status?: number };
        if (err.status === 401) {
          notifyRuntimeTransient(t('banner.unauthorized401'));
        } else {
          toast.error(t('banner.loadSessionFailed', { message: err.message }));
        }
        reconcileRuntimeAfterFetchFailure();
      }
    },
    [
      activeSessionIdRef,
      resumedThreadIdRef,
      threadTurnRef,
      threadContextSnapshotRef,
      threadContextCacheRef,
      messagesRef,
      abortThreadStream,
      resetTurnPersistState,
      setMessages,
      setActiveSessionId,
      setResumedThreadId,
      setRuntimeSessionEstablished,
      setThreadTrustMode,
      setPanelPreview,
      setThreadDetailForContext,
      setLastTurnOutputTokens,
      setContextWindowTokens,
      setSelectedWorkspace,
      refreshThreadContext,
      restoreThreadContextFromCache,
      reconcileRuntimeAfterFetchFailure,
      notifyRuntimeTransient,
      selectedModel,
      t,
    ],
  );

  const handleNewSession = useCallback(() => {
    abortThreadStream(resumedThreadIdRef.current);
    selectSessionAbortRef.current?.abort();
    selectSessionGenerationRef.current += 1;
    setMessages([]);
    setResumedThreadId(null);
    setLockedThreadTaskType(null);
    setThreadTrustMode(false);
    setPanelPreview(null);
    setActiveSessionId(null);
    setThreadDetailForContext(null);
    setLastTurnOutputTokens(null);
    setContextWindowTokens(contextWindowTokensForModel(selectedModel));
    try {
      localStorage.removeItem(ACTIVE_SESSION_STORAGE_KEY);
    } catch {
      /* ignore */
    }
    threadTurnRef.current = { threadId: '', turnId: '' };
    resetTurnPersistState();
    clearApproval();
  }, [
    abortThreadStream,
    clearApproval,
    resetTurnPersistState,
    resumedThreadIdRef,
    selectedModel,
    setActiveSessionId,
    setContextWindowTokens,
    setLastTurnOutputTokens,
    setLockedThreadTaskType,
    setMessages,
    setPanelPreview,
    setResumedThreadId,
    setThreadDetailForContext,
    setThreadTrustMode,
    threadTurnRef,
  ]);

  return {
    handleSelectSession,
    handleNewSession,
  };
}
