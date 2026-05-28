import { useCallback, useState, type Dispatch, type MutableRefObject, type SetStateAction } from 'react';
import {
  fetchJson,
  forkThreadAtUserMessage,
  getSessionDetail,
  getThreadDetail,
} from '../api/client';
import type { ComposerOutboundMessage } from '../components/Composer';
import { rebuildMessagesFromThreadEvents } from '../lib/chat/rebuildMessagesFromThread';
import { depthFromTailForUserMessage } from '../lib/chat/backtrackDepth';
import { cacheSessionUiMessages, type CachedUiMessage } from '../lib/chat/sessionUiCache';
import type { ThreadDetailWithTurns } from '../lib/contextUsage';
import { toast } from '../lib/toast';
import { usageRecordCacheHitPercent } from '../lib/cacheUsage';
import type { TurnChatMessage } from './useTurnSend';
type EditDraft = { messageId: string; content: string };

type BacktrackDraft = {
  messageId: string;
  content: string;
  depthFromTail: number;
};

export type UseChatMessageActionsParams = {
  t: (key: string, params?: Record<string, string>) => string;
  streaming: boolean;
  resumedThreadId: string | null;
  activeSessionId: string | null;
  messages: TurnChatMessage[];
  activeSessionIdRef: MutableRefObject<string | null>;
  resumedThreadIdRef: MutableRefObject<string | null>;
  threadTurnRef: MutableRefObject<{ threadId: string; turnId: string }>;
  streamControllersRef: MutableRefObject<Map<string, AbortController>>;
  sessionUiCacheRef: MutableRefObject<Map<string, CachedUiMessage[]>>;
  handleSend: (
    message: ComposerOutboundMessage,
    opts?: { editFromMessageId?: string },
  ) => void;
  setMessages: Dispatch<SetStateAction<TurnChatMessage[]>>;
  setResumedThreadId: Dispatch<SetStateAction<string | null>>;
  setStreamingThreadIds: Dispatch<SetStateAction<Set<string>>>;
  setPendingComposerStream: Dispatch<SetStateAction<boolean>>;
  setThreadDetailForContext: Dispatch<SetStateAction<ThreadDetailWithTurns | null>>;
  setLastTurnOutputTokens: Dispatch<SetStateAction<number | null>>;
  setLastCacheHitPercent: Dispatch<SetStateAction<number | null>>;
  setComposerPrefill: Dispatch<
    SetStateAction<{ text: string; nonce: number } | undefined>
  >;
  resetAgentPanel: () => void;
  resetTurnPersistState: () => void;
  refreshThreadContext: (threadId: string) => Promise<void>;
};

export function useChatMessageActions({
  t,
  streaming,
  resumedThreadId,
  activeSessionId,
  messages,
  activeSessionIdRef,
  resumedThreadIdRef,
  threadTurnRef,
  streamControllersRef,
  sessionUiCacheRef,
  handleSend,
  setMessages,
  setResumedThreadId,
  setStreamingThreadIds,
  setPendingComposerStream,
  setThreadDetailForContext,
  setLastTurnOutputTokens,
  setLastCacheHitPercent,
  setComposerPrefill,
  resetAgentPanel,
  resetTurnPersistState,
  refreshThreadContext,
}: UseChatMessageActionsParams) {
  const [editDraft, setEditDraft] = useState<EditDraft | null>(null);
  const [backtrackDraft, setBacktrackDraft] = useState<BacktrackDraft | null>(null);
  const [backtrackBusy, setBacktrackBusy] = useState(false);

  const handleExportSessionJson = useCallback(async () => {
    if (!activeSessionId) {
      toast.warning(t('banner.exportNoSession'));
      return;
    }
    const sid = activeSessionId;
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const { invoke } = await import('@tauri-apps/api/core');
      const savePath = await save({
        title: t('composer.exportSessionTitle'),
        defaultPath: `deepseek-session-${sid.slice(0, 8)}.json`,
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!savePath) return;
      await invoke('export_session_json', { sessionId: sid, savePath });
    } catch {
      try {
        const data = await getSessionDetail(sid);
        const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `deepseek-session-${sid.slice(0, 8)}.json`;
        a.click();
        URL.revokeObjectURL(url);
      } catch {
        toast.error(t('banner.exportNoData'));
      }
    }
  }, [activeSessionId, t]);

  const handleExportThreadJson = useCallback(async () => {
    if (!resumedThreadId) {
      toast.warning(t('banner.exportThreadNoId'));
      return;
    }
    const tid = resumedThreadId;
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const { invoke } = await import('@tauri-apps/api/core');
      const savePath = await save({
        title: t('composer.exportThreadTitle'),
        defaultPath: `deepseek-thread-${tid.slice(0, 8)}.json`,
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!savePath) return;
      await invoke('export_thread_json', { threadId: tid, savePath });
    } catch {
      try {
        const data = await fetchJson(`/v1/threads/${encodeURIComponent(tid)}`);
        const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `deepseek-thread-${tid.slice(0, 8)}.json`;
        a.click();
        URL.revokeObjectURL(url);
      } catch {
        toast.error(t('banner.exportThreadNoData'));
      }
    }
  }, [resumedThreadId, t]);

  const handleEditMessage = useCallback(
    (messageId: string, content: string) => {
      if (streaming || !resumedThreadId) {
        toast.warning(t('chat.editNeedsThread'));
        return;
      }
      const userMsgs = messages.filter((m) => m.role === 'user');
      const lastUser = userMsgs[userMsgs.length - 1];
      if (!lastUser || lastUser.id !== messageId) {
        toast.warning(t('chat.editLastOnly'));
        return;
      }
      setEditDraft({ messageId, content });
    },
    [streaming, resumedThreadId, messages, t],
  );

  const handleConfirmEdit = useCallback(() => {
    if (!editDraft?.content.trim()) {
      setEditDraft(null);
      return;
    }
    const draft = editDraft;
    setEditDraft(null);
    handleSend(
      { displayContent: draft.content.trim(), apiPrompt: draft.content.trim() },
      { editFromMessageId: draft.messageId },
    );
  }, [editDraft, handleSend]);

  const handleBacktrackFromMessage = useCallback(
    (messageId: string, content: string) => {
      if (streaming || !resumedThreadId) {
        toast.warning(t('chat.backtrackNeedsThread'));
        return;
      }
      const depth = depthFromTailForUserMessage(messages, messageId);
      if (depth == null) {
        return;
      }
      setBacktrackDraft({ messageId, content, depthFromTail: depth });
    },
    [streaming, resumedThreadId, messages, t],
  );

  const handleConfirmBacktrack = useCallback(async () => {
    if (!backtrackDraft || !resumedThreadId || backtrackBusy) {
      return;
    }
    const sourceThreadId = resumedThreadId;
    const draft = backtrackDraft;
    setBacktrackDraft(null);
    setBacktrackBusy(true);
    try {
      const { thread, original_user_text } = await forkThreadAtUserMessage(
        sourceThreadId,
        draft.depthFromTail,
      );
      const newThreadId = thread.id;
      streamControllersRef.current.get(sourceThreadId)?.abort();
      streamControllersRef.current.delete(sourceThreadId);
      setStreamingThreadIds(new Set());
      setPendingComposerStream(false);
      resetAgentPanel();
      resumedThreadIdRef.current = newThreadId;
      setResumedThreadId(newThreadId);
      threadTurnRef.current = { threadId: newThreadId, turnId: '' };
      resetTurnPersistState();

      const rebuilt = await rebuildMessagesFromThreadEvents(newThreadId);
      setMessages(rebuilt);
      if (activeSessionIdRef.current) {
        cacheSessionUiMessages(sessionUiCacheRef.current, activeSessionIdRef.current, rebuilt);
      }

      const threadDetail = await getThreadDetail(newThreadId);
      setThreadDetailForContext(threadDetail);
      const turns = threadDetail.turns ?? [];
      const lastTurn = turns.length > 0 ? turns[turns.length - 1] : undefined;
      const lastOut = lastTurn?.usage?.output_tokens;
      setLastTurnOutputTokens(
        lastOut != null && Number.isFinite(lastOut) && lastOut > 0 ? lastOut : null,
      );
      setLastCacheHitPercent(usageRecordCacheHitPercent(lastTurn?.usage ?? null));
      void refreshThreadContext(newThreadId);

      const prefill = original_user_text?.trim();
      if (prefill) {
        setComposerPrefill({ text: prefill, nonce: Date.now() });
      }
      toast.success(t('chat.backtrackSuccess'));
    } catch (e) {
      toast.error(t('chat.backtrackFailed', { message: (e as Error).message }));
    } finally {
      setBacktrackBusy(false);
    }
  }, [
    backtrackDraft,
    resumedThreadId,
    backtrackBusy,
    refreshThreadContext,
    resetAgentPanel,
    resetTurnPersistState,
    t,
    streamControllersRef,
    setStreamingThreadIds,
    setPendingComposerStream,
    resumedThreadIdRef,
    setResumedThreadId,
    threadTurnRef,
    setMessages,
    activeSessionIdRef,
    sessionUiCacheRef,
    setThreadDetailForContext,
    setLastTurnOutputTokens,
    setLastCacheHitPercent,
    setComposerPrefill,
  ]);

  return {
    editDraft,
    setEditDraft,
    backtrackDraft,
    setBacktrackDraft,
    backtrackBusy,
    handleExportSessionJson,
    handleExportThreadJson,
    handleEditMessage,
    handleConfirmEdit,
    handleBacktrackFromMessage,
    handleConfirmBacktrack,
  };
}
