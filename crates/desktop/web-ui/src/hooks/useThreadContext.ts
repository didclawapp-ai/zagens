import { useCallback, useEffect, useMemo, useRef, useState, type MutableRefObject } from 'react';
import { getThreadContext } from '../api/client';
import {
  DEFAULT_CONTEXT_WINDOW_TOKENS,
  resolveContextUsedTokens,
  resolveContextUsagePercent,
  type ThreadContextSnapshot,
  type ThreadDetailWithTurns,
} from '../lib/contextUsage';
import { THREAD_CONTEXT_POLL_STREAMING_MS } from '../lib/runtimePoll';

type ContextMessage = {
  role: string;
  content: string;
  tools?: { input?: string; output?: string }[];
};

export type UseThreadContextParams = {
  messages: ContextMessage[];
  resumedThreadId: string | null;
  resumedThreadIdRef: MutableRefObject<string | null>;
  streaming: boolean;
};

export type UseThreadContextResult = {
  contextWindowTokens: number;
  setContextWindowTokens: React.Dispatch<React.SetStateAction<number>>;
  threadDetailForContext: ThreadDetailWithTurns | null;
  setThreadDetailForContext: React.Dispatch<React.SetStateAction<ThreadDetailWithTurns | null>>;
  threadContextSnapshot: ThreadContextSnapshot | null;
  threadContextSnapshotRef: MutableRefObject<ThreadContextSnapshot | null>;
  threadContextCacheRef: MutableRefObject<Map<string, ThreadContextSnapshot>>;
  applyThreadContextSnapshot: (threadId: string, snap: ThreadContextSnapshot) => void;
  restoreThreadContextFromCache: (threadId: string) => void;
  refreshThreadContext: (threadId: string) => Promise<void>;
  contextUsedTokens: number;
  contextUsagePct: number;
};

export function useThreadContext({
  messages,
  resumedThreadId,
  resumedThreadIdRef,
  streaming,
}: UseThreadContextParams): UseThreadContextResult {
  const [contextWindowTokens, setContextWindowTokens] = useState(DEFAULT_CONTEXT_WINDOW_TOKENS);
  const [threadDetailForContext, setThreadDetailForContext] = useState<ThreadDetailWithTurns | null>(
    null,
  );
  const [threadContextSnapshot, setThreadContextSnapshot] =
    useState<ThreadContextSnapshot | null>(null);

  const threadContextSnapshotRef = useRef<ThreadContextSnapshot | null>(null);
  const threadContextCacheRef = useRef<Map<string, ThreadContextSnapshot>>(new Map());

  useEffect(() => {
    threadContextSnapshotRef.current = threadContextSnapshot;
  }, [threadContextSnapshot]);

  const applyThreadContextSnapshot = useCallback((threadId: string, snap: ThreadContextSnapshot) => {
    threadContextCacheRef.current.set(threadId, snap);
    if (resumedThreadIdRef.current !== threadId) {
      return;
    }
    setThreadContextSnapshot(snap);
    setContextWindowTokens(snap.context_window_tokens);
  }, [resumedThreadIdRef]);

  const restoreThreadContextFromCache = useCallback(
    (threadId: string) => {
      const cached = threadContextCacheRef.current.get(threadId);
      if (!cached || resumedThreadIdRef.current !== threadId) {
        return;
      }
      setThreadContextSnapshot(cached);
      setContextWindowTokens(cached.context_window_tokens);
    },
    [resumedThreadIdRef],
  );

  const refreshThreadContext = useCallback(
    async (threadId: string) => {
      try {
        const snap = await getThreadContext(threadId);
        applyThreadContextSnapshot(threadId, snap);
      } catch {
        if (resumedThreadIdRef.current !== threadId) {
          return;
        }
        restoreThreadContextFromCache(threadId);
      }
    },
    [applyThreadContextSnapshot, restoreThreadContextFromCache, resumedThreadIdRef],
  );

  useEffect(() => {
    if (!resumedThreadId) {
      setThreadContextSnapshot(null);
      return;
    }
    restoreThreadContextFromCache(resumedThreadId);
    if (!streaming) {
      void refreshThreadContext(resumedThreadId);
      const id = window.setInterval(
        () => void refreshThreadContext(resumedThreadId),
        THREAD_CONTEXT_POLL_STREAMING_MS,
      );
      return () => window.clearInterval(id);
    }
    return undefined;
  }, [resumedThreadId, streaming, refreshThreadContext, restoreThreadContextFromCache]);

  const contextUsedTokens = useMemo(
    () =>
      resolveContextUsedTokens(
        messages,
        threadDetailForContext,
        contextWindowTokens,
        threadContextSnapshot,
      ),
    [messages, threadDetailForContext, contextWindowTokens, threadContextSnapshot],
  );

  const contextUsagePct = useMemo(
    () => resolveContextUsagePercent(contextUsedTokens, contextWindowTokens, threadContextSnapshot),
    [contextUsedTokens, contextWindowTokens, threadContextSnapshot],
  );

  return {
    contextWindowTokens,
    setContextWindowTokens,
    threadDetailForContext,
    setThreadDetailForContext,
    threadContextSnapshot,
    threadContextSnapshotRef,
    threadContextCacheRef,
    applyThreadContextSnapshot,
    restoreThreadContextFromCache,
    refreshThreadContext,
    contextUsedTokens,
    contextUsagePct,
  };
}
