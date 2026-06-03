import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from 'react';
import { stopThreadTurn } from '../api/turnControl';
import { toast } from '../lib/toast';

export type StreamSessionControl = {
  markInterrupted: () => void;
  finishOnce: () => void;
};

export type UseTurnStreamParams = {
  resumedThreadId: string | null;
  streamingRef: MutableRefObject<boolean>;
  t: (key: string, params?: Record<string, string>) => string;
  onCancelSideEffects: () => void;
  /** Clears detach/recovery state before runtime interrupt (wired by useTurnSend). */
  cancelCleanupRef?: MutableRefObject<(() => void) | null>;
};

export type UseTurnStreamResult = {
  streamingThreadIds: Set<string>;
  setStreamingThreadIds: Dispatch<SetStateAction<Set<string>>>;
  pendingComposerStream: boolean;
  setPendingComposerStream: Dispatch<SetStateAction<boolean>>;
  streaming: boolean;
  streamingRef: MutableRefObject<boolean>;
  streamControllersRef: MutableRefObject<Map<string, AbortController>>;
  threadTurnRef: MutableRefObject<{ threadId: string; turnId: string }>;
  streamSessionRef: MutableRefObject<StreamSessionControl | null>;
  abortThreadStream: (threadId: string | null | undefined) => void;
  handleCancelStream: () => void;
};

export function useTurnStream({
  resumedThreadId,
  streamingRef,
  t,
  onCancelSideEffects,
  cancelCleanupRef,
}: UseTurnStreamParams): UseTurnStreamResult {
  const [streamingThreadIds, setStreamingThreadIds] = useState<Set<string>>(() => new Set());
  const [pendingComposerStream, setPendingComposerStream] = useState(false);

  const streaming = useMemo(() => {
    if (pendingComposerStream) return true;
    const tid = resumedThreadId;
    return Boolean(tid && streamingThreadIds.has(tid));
  }, [pendingComposerStream, resumedThreadId, streamingThreadIds]);

  const streamControllersRef = useRef<Map<string, AbortController>>(new Map());
  const threadTurnRef = useRef<{ threadId: string; turnId: string }>({
    threadId: '',
    turnId: '',
  });
  const streamSessionRef = useRef<StreamSessionControl | null>(null);

  useEffect(() => {
    streamingRef.current = streaming;
  }, [streaming, streamingRef]);

  const abortThreadStream = useCallback((threadId: string | null | undefined) => {
    if (!threadId) return;
    streamControllersRef.current.get(threadId)?.abort();
    streamControllersRef.current.delete(threadId);
    setStreamingThreadIds((prev) => {
      const next = new Set(prev);
      next.delete(threadId);
      return next;
    });
    setPendingComposerStream(false);
  }, []);

  const handleCancelStream = useCallback(() => {
    cancelCleanupRef?.current?.();
    const { threadId, turnId } = threadTurnRef.current;
    const streamControl =
      (threadId ? streamControllersRef.current.get(threadId) : undefined) ??
      streamControllersRef.current.get('__pending__') ??
      undefined;

    void stopThreadTurn({ threadId, turnId, streamControl }).catch((e) => {
      const err = e as Error & { status?: number };
      if (err.status === 409) {
        return;
      }
      toast.warning(t('composer.interruptFailed', { message: err.message || String(e) }));
    });

    const session = streamSessionRef.current;
    if (session) {
      session.markInterrupted();
      session.finishOnce();
    } else {
      setStreamingThreadIds(new Set());
      setPendingComposerStream(false);
    }

    onCancelSideEffects();
  }, [cancelCleanupRef, onCancelSideEffects, t]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || e.ctrlKey || e.metaKey || e.altKey) return;
      const tag = (e.target as HTMLElement)?.tagName?.toLowerCase();
      if (tag === 'input' || tag === 'textarea' || tag === 'select') return;
      if (!streamingRef.current) return;
      e.preventDefault();
      handleCancelStream();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [handleCancelStream]);

  return {
    streamingThreadIds,
    setStreamingThreadIds,
    pendingComposerStream,
    setPendingComposerStream,
    streaming,
    streamingRef,
    streamControllersRef,
    threadTurnRef,
    streamSessionRef,
    abortThreadStream,
    handleCancelStream,
  };
}
