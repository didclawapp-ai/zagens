import { useCallback, useEffect, useMemo, useRef, useState, type MutableRefObject } from 'react';
import { getThreadContext, getThreadContextBreakdown } from '../api/client';
import {
  DEFAULT_CONTEXT_WINDOW_TOKENS,
  resolveContextUsedTokens,
  resolveContextUsagePercent,
  type ContextUsageBreakdown,
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
  threadContextUsage: ContextUsageBreakdown | null;
  threadContextUsageRef: MutableRefObject<ContextUsageBreakdown | null>;
  applyThreadContextSnapshot: (threadId: string, snap: ThreadContextSnapshot) => void;
  applyContextUsageBreakdown: (threadId: string, breakdown: ContextUsageBreakdown) => void;
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
  const [threadContextUsage, setThreadContextUsage] = useState<ContextUsageBreakdown | null>(null);

  const threadContextSnapshotRef = useRef<ThreadContextSnapshot | null>(null);
  const threadContextCacheRef = useRef<Map<string, ThreadContextSnapshot>>(new Map());
  const threadContextUsageRef = useRef<ContextUsageBreakdown | null>(null);
  const threadContextUsageCacheRef = useRef<Map<string, ContextUsageBreakdown>>(new Map());

  useEffect(() => {
    threadContextSnapshotRef.current = threadContextSnapshot;
  }, [threadContextSnapshot]);

  useEffect(() => {
    threadContextUsageRef.current = threadContextUsage;
  }, [threadContextUsage]);

  const applyThreadContextSnapshot = useCallback((threadId: string, snap: ThreadContextSnapshot) => {
    threadContextCacheRef.current.set(threadId, snap);
    if (resumedThreadIdRef.current !== threadId) {
      return;
    }
    setThreadContextSnapshot(snap);
    setContextWindowTokens(snap.context_window_tokens);
  }, [resumedThreadIdRef]);

  const applyContextUsageBreakdown = useCallback(
    (threadId: string, breakdown: ContextUsageBreakdown) => {
      threadContextUsageCacheRef.current.set(threadId, breakdown);
      if (resumedThreadIdRef.current !== threadId) {
        return;
      }
      setThreadContextUsage(breakdown);
      if (breakdown.context_window_tokens > 0) {
        setContextWindowTokens(breakdown.context_window_tokens);
      }
    },
    [resumedThreadIdRef],
  );

  const restoreThreadContextFromCache = useCallback(
    (threadId: string) => {
      if (resumedThreadIdRef.current !== threadId) {
        return;
      }
      const cached = threadContextCacheRef.current.get(threadId);
      if (cached) {
        setThreadContextSnapshot(cached);
        setContextWindowTokens(cached.context_window_tokens);
      }
      const cachedUsage = threadContextUsageCacheRef.current.get(threadId);
      if (cachedUsage) {
        setThreadContextUsage(cachedUsage);
        if (cachedUsage.context_window_tokens > 0) {
          setContextWindowTokens(cachedUsage.context_window_tokens);
        }
      }
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
        const cached = threadContextCacheRef.current.get(threadId);
        if (cached) {
          setThreadContextSnapshot(cached);
        }
      }
      try {
        const breakdown = await getThreadContextBreakdown(threadId);
        applyContextUsageBreakdown(threadId, breakdown);
      } catch {
        if (resumedThreadIdRef.current !== threadId) {
          return;
        }
        const cachedUsage = threadContextUsageCacheRef.current.get(threadId);
        if (cachedUsage) {
          setThreadContextUsage(cachedUsage);
        }
      }
    },
    [applyContextUsageBreakdown, applyThreadContextSnapshot, resumedThreadIdRef],
  );

  useEffect(() => {
    if (!resumedThreadId) {
      setThreadContextSnapshot(null);
      setThreadContextUsage(null);
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

  const contextUsedTokens = useMemo(() => {
    if (threadContextUsage && threadContextUsage.estimated_input_tokens > 0) {
      return Math.min(
        threadContextUsage.estimated_input_tokens,
        threadContextUsage.context_window_tokens || contextWindowTokens,
      );
    }
    return resolveContextUsedTokens(
      messages,
      threadDetailForContext,
      contextWindowTokens,
      threadContextSnapshot,
    );
  }, [
    messages,
    threadDetailForContext,
    contextWindowTokens,
    threadContextSnapshot,
    threadContextUsage,
  ]);

  const contextUsagePct = useMemo(() => {
    if (threadContextUsage && threadContextUsage.usage_percent > 0) {
      return threadContextUsage.usage_percent;
    }
    return resolveContextUsagePercent(contextUsedTokens, contextWindowTokens, threadContextSnapshot);
  }, [contextUsedTokens, contextWindowTokens, threadContextSnapshot, threadContextUsage]);

  return {
    contextWindowTokens,
    setContextWindowTokens,
    threadDetailForContext,
    setThreadDetailForContext,
    threadContextSnapshot,
    threadContextSnapshotRef,
    threadContextCacheRef,
    threadContextUsage,
    threadContextUsageRef,
    applyThreadContextSnapshot,
    applyContextUsageBreakdown,
    restoreThreadContextFromCache,
    refreshThreadContext,
    contextUsedTokens,
    contextUsagePct,
  };
}
