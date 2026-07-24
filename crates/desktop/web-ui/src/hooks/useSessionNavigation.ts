import {
  useCallback,
  useRef,
  useState,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from 'react';
import {
  getSessionDetail,
  getThreadDetail,
  resumeSessionThread,
} from '../api/client';
import { persistThreadSessionDeduped } from '../lib/chat/persistThreadSessionDedup';
import { rebuildMessagesFromThreadEvents } from '../lib/chat/rebuildMessagesFromThread';
import { mergeThreadTranscript } from './turnSend/completeStreamUi';
import {
  pickBestSessionMessagesWithSource,
  snapshotHasAssistantMeta,
  type SessionMessageCandidate,
} from '../lib/chat/sessionMessagePick';
import {
  cacheSessionUiMessages,
  getCachedSessionUiMessages,
  type CachedUiMessage,
} from '../lib/chat/sessionUiCache';
import { mapSessionDetailToMessages } from '../lib/chat/sessionMessages';
import { applyStreamingReattach } from '../lib/chat/sessionStreamReattach';
import {
  contextWindowTokensForModel,
  type ThreadContextSnapshot,
} from '../lib/contextUsage';
import type { PreviewState } from '../components/preview/types';
import { toast } from '../lib/toast';
import type { StreamContextRegistry } from './useStreamContextRegistry';
import { readThreadTurn, writeThreadTurn } from '../lib/chat/streamContextAccess';
import type { ApprovalState } from './useTurnApproval';
import {
  registerWindowThread,
  saveStoredActiveSessionId,
  clearStoredActiveSessionId,
} from '../lib/windowBridge';
import { usageRecordCacheHitPercent } from '../lib/cacheUsage';
import type { ComposerModelId, DesktopTaskTypeResolved } from '../types/desktop';
import type { LhtChipState } from '../lib/lhtChip';

type NavMessage = CachedUiMessage;

export type UseSessionNavigationParams = {
  t: (key: string, params?: Record<string, string>) => string;
  selectedModel: ComposerModelId;
  activeSessionIdRef: MutableRefObject<string | null>;
  resumedThreadIdRef: MutableRefObject<string | null>;
  streamRegistry: StreamContextRegistry;
  threadContextSnapshotRef: MutableRefObject<ThreadContextSnapshot | null>;
  threadContextCacheRef: MutableRefObject<Map<string, ThreadContextSnapshot>>;
  messagesRef: MutableRefObject<NavMessage[]>;
  sessionUiCacheRef: MutableRefObject<Map<string, CachedUiMessage[]>>;
  abortThreadStream: (
    threadId: string | null | undefined,
    opts?: { clearComposerLock?: boolean },
  ) => void;
  /**
   * Multi-session P0.4: when provided, navigating away from a thread that is
   * still in this set skips `abortThreadStream` so the turn keeps streaming in
   * the background. Omit to keep the legacy "abort on navigate" behaviour.
   */
  streamingThreadIdsRef?: MutableRefObject<Set<string>>;
  streamControllersRef?: MutableRefObject<Map<string, AbortController>>;
  /** threadId → sessionId for SessionStrip streaming indicators. */
  bindThreadSession?: (threadId: string, sessionId: string | null | undefined) => void;
  desktopHost?: boolean;
  setPendingComposerStream?: Dispatch<SetStateAction<boolean>>;
  showApprovalIfOwned?: (desktopHost: boolean, payload: ApprovalState) => void;
  setLhtChip?: Dispatch<SetStateAction<LhtChipState | null>>;
  applyThreadContextSnapshot?: (threadId: string, snapshot: ThreadContextSnapshot) => void;
  refreshSessions?: () => Promise<void>;
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
  setLastCacheHitPercent: Dispatch<SetStateAction<number | null>>;
  setContextWindowTokens: Dispatch<SetStateAction<number>>;
  setSelectedWorkspace: Dispatch<SetStateAction<string>>;
  setLockedThreadTaskType: Dispatch<SetStateAction<DesktopTaskTypeResolved | null>>;
  refreshThreadContext: (threadId: string) => Promise<void>;
  restoreThreadContextFromCache: (threadId: string) => void;
  reconcileRuntimeAfterFetchFailure: () => void;
  notifyRuntimeTransient: (message: string) => void;
  resetAgentPanel: () => void;
};

export type SessionRestoreSource = SessionMessageCandidate['source'] | null;

export type UseSessionNavigationResult = {
  handleSelectSession: (sessionId: string) => Promise<void>;
  handleNewSession: () => void;
  handleOpenThreadById: (threadId: string) => Promise<void>;
  sessionRestoreLoading: boolean;
  sessionRestoreSource: SessionRestoreSource;
  retrySessionRestore: () => Promise<void>;
};

export function useSessionNavigation({
  t,
  selectedModel,
  activeSessionIdRef,
  resumedThreadIdRef,
  streamRegistry,
  threadContextSnapshotRef,
  threadContextCacheRef,
  messagesRef,
  sessionUiCacheRef,
  abortThreadStream,
  streamingThreadIdsRef,
  streamControllersRef,
  bindThreadSession,
  desktopHost = false,
  setPendingComposerStream,
  showApprovalIfOwned,
  setLhtChip,
  applyThreadContextSnapshot,
  refreshSessions,
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
  setLastCacheHitPercent,
  setContextWindowTokens,
  setSelectedWorkspace,
  setLockedThreadTaskType,
  refreshThreadContext,
  restoreThreadContextFromCache,
  reconcileRuntimeAfterFetchFailure,
  notifyRuntimeTransient,
  resetAgentPanel,
}: UseSessionNavigationParams): UseSessionNavigationResult {
  // Multi-session P0.4: when the outgoing thread is still streaming, detach
  // (keep its SSE alive in the background) instead of aborting. The thread's
  // events route into its background `StreamContext` via `useTurnSend`'s
  // `isBackground` guard.
  const notifyBackgroundStreamDetached = useCallback(
    (threadId: string) => {
      const tid = threadId.trim();
      if (!tid) return;
      const sid = streamRegistry?.getContext(tid)?.sessionId;
      const label = sid?.slice(0, 8) ?? tid.slice(0, 8);
      toast.info(t('composer.bgStreamRunning', { thread: label }), {
        tag: `bg-stream-${tid}`,
        duration: 5000,
      });
    },
    [streamRegistry, t],
  );

  const detachOrAbort = useCallback(
    (threadId: string | null | undefined) => {
      if (!threadId) return;
      if (streamingThreadIdsRef?.current.has(threadId)) {
        notifyBackgroundStreamDetached(threadId);
        return;
      }
      // turn_started may not have fired yet; an armed controller still means detach.
      if (streamControllersRef?.current.has(threadId)) {
        notifyBackgroundStreamDetached(threadId);
        return;
      }
      abortThreadStream(threadId, { clearComposerLock: false });
    },
    [
      abortThreadStream,
      notifyBackgroundStreamDetached,
      streamControllersRef,
      streamingThreadIdsRef,
    ],
  );

  const persistOutgoingThread = useCallback(
    (threadId: string | null | undefined, sessionId: string | null | undefined) => {
      const tid = threadId?.trim();
      if (!tid) return;
      const knownSid =
        sessionId ??
        streamRegistry?.getContext(tid)?.sessionId ??
        null;
      void persistThreadSessionDeduped(tid, knownSid)
        .then(async (res) => {
          bindThreadSession?.(tid, res.session_id);
          await refreshSessions?.();
        })
        .catch(() => {
          /* best-effort — turn_started persist or checkpoint will retry */
        });
    },
    [bindThreadSession, refreshSessions, streamRegistry],
  );

  const reattachStreamingIfNeeded = useCallback(
    async (
      threadId: string,
      messages: NavMessage[],
      sessionId: string | null,
    ): Promise<NavMessage[]> => {
      if (!streamingThreadIdsRef) {
        return messages;
      }
      const reattach = await applyStreamingReattach(threadId, messages, {
        streamRegistry,
        setLhtChip,
        applyThreadContextSnapshot,
      });
      if (reattach.composerLocked) {
        setPendingComposerStream?.(true);
      }
      if (reattach.pendingApproval && showApprovalIfOwned) {
        toast.dismissByTag(`bg-approval-${threadId}`);
        showApprovalIfOwned(desktopHost, reattach.pendingApproval);
        streamRegistry?.patchContext(threadId, { pendingApproval: null });
      }
      if (sessionId && reattach.messages.length > 0) {
        cacheSessionUiMessages(sessionUiCacheRef.current, sessionId, reattach.messages);
      }
      return reattach.messages;
    },
    [
      desktopHost,
      sessionUiCacheRef,
      setPendingComposerStream,
      showApprovalIfOwned,
      setLhtChip,
      applyThreadContextSnapshot,
      streamRegistry,
    ],
  );

  const selectSessionGenerationRef = useRef(0);
  const selectSessionAbortRef = useRef<AbortController | null>(null);
  const [sessionRestoreLoading, setSessionRestoreLoading] = useState(false);
  const [sessionRestoreSource, setSessionRestoreSource] =
    useState<SessionRestoreSource>(null);

  const retrySessionRestore = useCallback(async () => {
    const threadId = resumedThreadIdRef.current;
    const sessionId = activeSessionIdRef.current;
    if (!threadId) {
      return;
    }
    setSessionRestoreLoading(true);
    try {
      const fromThread = await rebuildMessagesFromThreadEvents(threadId);
      if (fromThread.length > 0) {
        setMessages(fromThread);
        setSessionRestoreSource('thread');
        if (sessionId) {
          cacheSessionUiMessages(sessionUiCacheRef.current, sessionId, fromThread);
        }
      }
    } catch {
      notifyRuntimeTransient(t('banner.sessionRestoreRetryFailed'));
    } finally {
      setSessionRestoreLoading(false);
    }
  }, [
    activeSessionIdRef,
    resumedThreadIdRef,
    sessionUiCacheRef,
    setMessages,
    notifyRuntimeTransient,
    t,
  ]);

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
      if (outgoingThreadId) {
        persistOutgoingThread(outgoingThreadId, outgoingSessionId);
      }
      const outgoingSnapshot = threadContextSnapshotRef.current;
      if (outgoingThreadId && outgoingSnapshot) {
        threadContextCacheRef.current.set(outgoingThreadId, outgoingSnapshot);
      }
      if (outgoingThreadId) {
        detachOrAbort(outgoingThreadId);
      }

      toast.dismissAll();
      resetAgentPanel();
      setPendingComposerStream?.(false);
      setActiveSessionId(sessionId);
      setResumedThreadId(null);
      setThreadTrustMode(false);
      setPanelPreview(null);
      resetTurnPersistState();
      setSessionRestoreLoading(true);
      setSessionRestoreSource(null);

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
        const restoreCandidates: SessionMessageCandidate[] = [];
        if (cachedUi?.length) {
          restoreCandidates.push({ source: 'cache', messages: cachedUi });
        }
        if (sessionFallback.length > 0) {
          restoreCandidates.push({ source: 'session', messages: sessionFallback });
        }
        const provisional = pickBestSessionMessagesWithSource(restoreCandidates);
        if (provisional.messages.length > 0) {
          setMessages(provisional.messages);
          setSessionRestoreSource(provisional.source);
        }
        resumedThreadIdRef.current = resumed.thread_id;
        setResumedThreadId(resumed.thread_id);
        bindThreadSession?.(resumed.thread_id, sessionId);
        setRuntimeSessionEstablished(true);
        restoreThreadContextFromCache(resumed.thread_id);
        try {
          let fromThread = await rebuildMessagesFromThreadEvents(resumed.thread_id, {
            signal: selectAbort.signal,
          });
          if (gen !== selectSessionGenerationRef.current) {
            return;
          }
          const ctxMessages = streamRegistry.getContext(resumed.thread_id)?.messages ?? [];
          if (fromThread.length > 0 && ctxMessages.length > 0) {
            fromThread = mergeThreadTranscript(
              ctxMessages as import('./useTurnSend').TurnChatMessage[],
              fromThread as import('./useTurnSend').TurnChatMessage[],
            );
          }
          if (fromThread.length > 0) {
            restoreCandidates.push({ source: 'thread', messages: fromThread });
          }
        } catch {
          /* thread replay failed — keep cache/session fallback */
        }
        if (gen !== selectSessionGenerationRef.current) {
          return;
        }
        let picked = pickBestSessionMessagesWithSource(restoreCandidates);
        const threadCandidate = restoreCandidates.find((c) => c.source === 'thread');
        if (
          threadCandidate &&
          threadCandidate.messages.length > 0 &&
          snapshotHasAssistantMeta(threadCandidate.messages) &&
          !snapshotHasAssistantMeta(picked.messages)
        ) {
          picked = { messages: threadCandidate.messages, source: 'thread' };
        }
        if (picked.messages.length > 0) {
          setSessionRestoreSource(picked.source);
        }

        const reattachedMessages = await reattachStreamingIfNeeded(
          resumed.thread_id,
          picked.messages.length > 0 ? picked.messages : [],
          sessionId,
        );
        if (gen !== selectSessionGenerationRef.current) {
          return;
        }
        if (reattachedMessages.length > 0) {
          setMessages(reattachedMessages);
          if (picked.source) {
            setSessionRestoreSource(picked.source);
          }
          cacheSessionUiMessages(sessionUiCacheRef.current, sessionId, reattachedMessages);
        }
        try {
          const threadDetail = await getThreadDetail(resumed.thread_id);
          if (gen !== selectSessionGenerationRef.current) {
            return;
          }
          setThreadDetailForContext(threadDetail);
          const turns = threadDetail.turns ?? [];
          const lastTurn = turns.length > 0 ? turns[turns.length - 1] : undefined;
          const latestTurnId =
            threadDetail.thread.latest_turn_id?.trim() ||
            lastTurn?.id?.trim() ||
            readThreadTurn(streamRegistry, resumed.thread_id).turnId ||
            '';
          if (latestTurnId) {
            writeThreadTurn(streamRegistry, resumed.thread_id, latestTurnId);
          }
          const lastOut = lastTurn?.usage?.output_tokens;
          setLastTurnOutputTokens(
            lastOut != null && Number.isFinite(lastOut) && lastOut > 0 ? lastOut : null,
          );
          setLastCacheHitPercent(usageRecordCacheHitPercent(lastTurn?.usage ?? null));
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
        saveStoredActiveSessionId(sessionId);
        void refreshSessions?.();
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
      } finally {
        if (gen === selectSessionGenerationRef.current) {
          setSessionRestoreLoading(false);
        }
      }
    },
    [
      activeSessionIdRef,
      resumedThreadIdRef,
      threadContextSnapshotRef,
      threadContextCacheRef,
      messagesRef,
      detachOrAbort,
      reattachStreamingIfNeeded,
      bindThreadSession,
      persistOutgoingThread,
      refreshSessions,
      resetAgentPanel,
      resetTurnPersistState,
      setMessages,
      setActiveSessionId,
      setResumedThreadId,
      setRuntimeSessionEstablished,
      setThreadTrustMode,
      setPanelPreview,
      setThreadDetailForContext,
      setLastTurnOutputTokens,
      setLastCacheHitPercent,
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

  const handleOpenThreadById = useCallback(
    async (threadId: string) => {
      const trimmed = threadId.trim();
      if (!trimmed) {
        return;
      }
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
      if (outgoingThreadId) {
        persistOutgoingThread(outgoingThreadId, outgoingSessionId);
      }
      const outgoingSnapshot = threadContextSnapshotRef.current;
      if (outgoingThreadId && outgoingSnapshot) {
        threadContextCacheRef.current.set(outgoingThreadId, outgoingSnapshot);
      }
      if (outgoingThreadId) {
        detachOrAbort(outgoingThreadId);
      }

      toast.dismissAll();
      resetAgentPanel();
      setActiveSessionId(null);
      clearStoredActiveSessionId();
      setResumedThreadId(trimmed);
      resumedThreadIdRef.current = trimmed;
      setLockedThreadTaskType(null);
      setThreadTrustMode(false);
      setPanelPreview(null);
      resetTurnPersistState();
      setMessages([]);
      setRuntimeSessionEstablished(true);
      restoreThreadContextFromCache(trimmed);

      try {
        let fromThread = await rebuildMessagesFromThreadEvents(trimmed, {
          signal: selectAbort.signal,
        });
        if (gen !== selectSessionGenerationRef.current) {
          return;
        }
        const ctxMessages = streamRegistry.getContext(trimmed)?.messages ?? [];
        if (fromThread.length > 0 && ctxMessages.length > 0) {
          fromThread = mergeThreadTranscript(
            ctxMessages as import('./useTurnSend').TurnChatMessage[],
            fromThread as import('./useTurnSend').TurnChatMessage[],
          );
        }
        if (fromThread.length > 0) {
          const reattached = await reattachStreamingIfNeeded(trimmed, fromThread, null);
          if (gen !== selectSessionGenerationRef.current) {
            return;
          }
          setMessages(reattached);
        } else {
          const reattached = await reattachStreamingIfNeeded(trimmed, [], null);
          if (gen !== selectSessionGenerationRef.current) {
            return;
          }
          if (reattached.length > 0) {
            setMessages(reattached);
          }
        }
        const threadDetail = await getThreadDetail(trimmed);
        if (gen !== selectSessionGenerationRef.current) {
          return;
        }
        setThreadDetailForContext(threadDetail);
        const turns = threadDetail.turns ?? [];
        const lastTurn = turns.length > 0 ? turns[turns.length - 1] : undefined;
        const latestTurnId =
          threadDetail.thread.latest_turn_id?.trim() ||
          lastTurn?.id?.trim() ||
          readThreadTurn(streamRegistry, trimmed).turnId ||
          '';
        if (latestTurnId) {
          writeThreadTurn(streamRegistry, trimmed, latestTurnId);
        }
        const lastOut = lastTurn?.usage?.output_tokens;
        setLastTurnOutputTokens(
          lastOut != null && Number.isFinite(lastOut) && lastOut > 0 ? lastOut : null,
        );
        setLastCacheHitPercent(usageRecordCacheHitPercent(lastTurn?.usage ?? null));
        setContextWindowTokens(
          contextWindowTokensForModel(threadDetail.thread.model ?? selectedModel),
        );
        setSelectedWorkspace(threadDetail.thread.workspace);
        setThreadTrustMode(Boolean(threadDetail.thread.trust_mode));
        void registerWindowThread(trimmed);
        if (gen === selectSessionGenerationRef.current) {
          void refreshThreadContext(trimmed);
        }
      } catch (e) {
        if (gen !== selectSessionGenerationRef.current) {
          return;
        }
        const err = e as Error & { status?: number };
        if (err.status === 401) {
          notifyRuntimeTransient(t('banner.unauthorized401'));
        } else {
          toast.error(t('automation.openInChatFailed', { message: err.message }));
        }
        reconcileRuntimeAfterFetchFailure();
      }
    },
    [
      activeSessionIdRef,
      resumedThreadIdRef,
      threadContextSnapshotRef,
      threadContextCacheRef,
      messagesRef,
      sessionUiCacheRef,
      detachOrAbort,
      reattachStreamingIfNeeded,
      resetTurnPersistState,
      resetAgentPanel,
      setMessages,
      setActiveSessionId,
      setResumedThreadId,
      setRuntimeSessionEstablished,
      setThreadTrustMode,
      setPanelPreview,
      setThreadDetailForContext,
      setLastTurnOutputTokens,
      setLastCacheHitPercent,
      setContextWindowTokens,
      setSelectedWorkspace,
      setLockedThreadTaskType,
      refreshThreadContext,
      restoreThreadContextFromCache,
      reconcileRuntimeAfterFetchFailure,
      notifyRuntimeTransient,
      selectedModel,
      t,
    ],
  );

  const handleNewSession = useCallback(() => {
    const outgoingThreadId = resumedThreadIdRef.current;
    const outgoingSessionId = activeSessionIdRef.current;
    if (outgoingThreadId) {
      persistOutgoingThread(outgoingThreadId, outgoingSessionId);
    }
    detachOrAbort(outgoingThreadId);
    selectSessionAbortRef.current?.abort();
    selectSessionGenerationRef.current += 1;
    setPendingComposerStream?.(false);
    resetAgentPanel();
    setMessages([]);
    setResumedThreadId(null);
    setLockedThreadTaskType(null);
    setThreadTrustMode(false);
    setPanelPreview(null);
    setActiveSessionId(null);
    setThreadDetailForContext(null);
    setLastTurnOutputTokens(null);
    setLastCacheHitPercent(null);
    setContextWindowTokens(contextWindowTokensForModel(selectedModel));
    clearStoredActiveSessionId();
    writeThreadTurn(streamRegistry, '', '');
    resetTurnPersistState();
    clearApproval();
    setSessionRestoreLoading(false);
    setSessionRestoreSource(null);
  }, [
    detachOrAbort,
    clearApproval,
    bindThreadSession,
    persistOutgoingThread,
    setPendingComposerStream,
    resetAgentPanel,
    resetTurnPersistState,
    resumedThreadIdRef,
    selectedModel,
    setActiveSessionId,
    setContextWindowTokens,
    setLastTurnOutputTokens,
    setLastCacheHitPercent,
    setLockedThreadTaskType,
    setMessages,
    setPanelPreview,
    setResumedThreadId,
    setThreadDetailForContext,
    setThreadTrustMode,
    streamRegistry,
  ]);

  return {
    handleSelectSession,
    handleNewSession,
    handleOpenThreadById,
    sessionRestoreLoading,
    sessionRestoreSource,
    retrySessionRestore,
  };
}
