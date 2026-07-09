import {
  useCallback,
  useEffect,
  useRef,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from 'react';
import {
  editLastThreadTurn,
  filterThreadStreamEvents,
  getThreadDetail,
  pollThreadTurnEvents,
  postStreamTurn,
  resumeSessionThread,
  sseEventSeq,
  startThreadTurn,
  threadIdFromSseEvent,
  threadTurnStillActive,
  type RuntimeConnectionState,
  type SseTurnEvent,
} from '../api/client';
import { rebuildMessagesFromThreadEvents } from '../lib/chat/rebuildMessagesFromThread';
import {
  applyStreamEventToMessages,
} from './turnSend/applyStreamEventToMessages';
import { reconcileMessagesFromThread, mergeThreadTranscript } from './turnSend/completeStreamUi';
import { createEmptyTimelineState } from '../lib/chat/timeline/turnTimelineReducer';
import type { TimelineState } from '../lib/chat/timeline/turnBlockTypes';
import { persistThreadSessionDeduped } from '../lib/chat/persistThreadSessionDedup';
import { sessionMessageRichness } from '../lib/chat/sessionMessagePick';
import type { ComposerOutboundMessage } from '../components/Composer';
import { normalizeDesktopStreamEvent, type NormalizedStreamEvent } from '../api/streamNormalize';
import { notifyCraftBlackboardChanged } from '../lib/craftBlackboard';
import { loadNotifyMethod } from '../lib/appPreferences';
import {
  capToolOutputForDisplay,
  mergeStreamingToolOutput,
  toolOutputString,
} from '../lib/chat/toolOutput';
import {
  cacheSessionUiMessages,
  type CachedUiMessage,
} from '../lib/chat/sessionUiCache';
import type { ContextUsageBreakdown, ThreadContextSnapshot } from '../lib/contextUsage';
import { normalizeContextUsageBreakdown } from '../components/ContextExplorerView';
import type { ModelParams } from '../components/ModelParamsDialog';
import { modelSamplingForApi } from '../lib/modelParams';
import {
  dispatchPanelChecklist,
  dispatchPanelContext,
  dispatchPanelContextUsage,
  dispatchPanelScratchpad,
  dispatchPanelTaskGraph,
  dispatchHarnessCycleAdvanced,
  normalizeChecklistPayload,
  setPanelActiveThreadId,
} from '../lib/panelChannel';
import type { HarnessTaskGraph } from '../lib/types/longHorizon';
import { streamFlagsForRunMode } from '../lib/runtimeMode';
import { toast } from '../lib/toast';
import { registerWindowThread } from '../lib/windowBridge';
import {
  resolveRouteIntentForApi,
  type ComposerModelId,
  type DesktopRouteIntentOption,
  type DesktopRunModeId,
  type DesktopTaskTypePreference,
} from '../types/desktop';
import type { ApprovalState } from './useTurnApproval';
import type { FinishOnceOptions } from './useTurnStream';
import { saveStoredActiveSessionId } from '../lib/windowBridge';
import { turnCacheHitPercent } from '../lib/cacheUsage';
import { parseLhtStatusMessage, type LhtChipState } from '../lib/lhtChip';
import {
  anyAssistantStreaming,
  clearStreamingAssistants,
  lastAssistantMessageId,
  rebindStreamingAssistant,
  resolveStreamTargetId,
} from '../lib/chat/activeTurnStreamUi';
import {
  isBackgroundStreamEvent,
} from '../lib/chat/streamContextStore';
import { getActiveThreadIdsFromStore } from '../lib/chat/threadStatusStore';
import {
  clearActiveStreamHandles,
  readThreadTurn,
  resolveThreadIdForSend,
  writeLiveDeliver,
  writeRecoveryCtx,
  writeStreamSession,
  writeThreadTurn,
} from '../lib/chat/streamContextAccess';
import type { ScratchpadStatus } from '../api/client';
import {
  useTurnStreamRecovery,
} from './useTurnStreamRecovery';
import type { StreamContextRegistry } from './useStreamContextRegistry';

export type TurnChatMessage = {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  thinking?: string;
  tools?: {
    id: string;
    name: string;
    input: string;
    output?: string;
    status: 'running' | 'done' | 'error';
  }[];
  blocks?: import('../lib/chat/timeline/turnBlockTypes').TurnBlock[];
  isStreaming?: boolean;
  /** Set when replay lacks persisted thinking segments (P1.1). */
  thinkingIncomplete?: boolean;
};

let msgId = 0;
function nextId() {
  return `msg-${++msgId}`;
}

// Monotonic counter for per-send stream keys. Replaces the shared `__pending__`
// key so that two concurrent sends (e.g. A completing while B is still pending
// its `turn_started`) cannot abort or delete each other's AbortController.
let sendSeq = 0;
function nextSendKey(): string {
  return `__send_${++sendSeq}__`;
}

/** Soft warning / hard block for concurrent live SSE consumers (S1.2). */
const MAX_CONCURRENT_STREAMING_WARN = 8;
const MAX_CONCURRENT_STREAMING_LIMIT = 12;

export type UseTurnSendParams = {
  t: (key: string, params?: Record<string, string>) => string;
  runtimeConn: RuntimeConnectionState;
  streaming: boolean;
  resumedThreadId: string | null;
  resumedThreadIdRef: MutableRefObject<string | null>;
  runMode: DesktopRunModeId;
  autoApprove: boolean;
  routeIntent: DesktopRouteIntentOption;
  selectedModel: ComposerModelId;
  selectedWorkspace: string;
  useWorktree: boolean;
  taskTypePreference: DesktopTaskTypePreference;
  modelParams: ModelParams;
  desktopHost: boolean;
  streamControllersRef: MutableRefObject<Map<string, AbortController>>;
  /** Per-send key for the pending controller (before turn_started resolves threadId). */
  pendingSendKeyRef: MutableRefObject<string | null>;
  setPendingComposerStream: Dispatch<SetStateAction<boolean>>;
  setMessages: Dispatch<SetStateAction<TurnChatMessage[]>>;
  setResumedThreadId: Dispatch<SetStateAction<string | null>>;
  setActiveSessionId: Dispatch<SetStateAction<string | null>>;
  setRuntimeSessionEstablished: Dispatch<SetStateAction<boolean>>;
  setLastTurnOutputTokens: Dispatch<SetStateAction<number | null>>;
  setLastCacheHitPercent: Dispatch<SetStateAction<number | null>>;
  setLhtChip: Dispatch<SetStateAction<LhtChipState | null>>;
  activeSessionIdRef: MutableRefObject<string | null>;
  sessionUiCacheRef: MutableRefObject<Map<string, CachedUiMessage[]>>;
  refreshSessions: () => Promise<void>;
  refreshThreadContext: (threadId: string) => Promise<void>;
  applyThreadContextSnapshot: (threadId: string, snap: ThreadContextSnapshot) => void;
  applyContextUsageBreakdown: (threadId: string, breakdown: ContextUsageBreakdown) => void;
  notifyRuntimeTransient: (message: string) => void;
  resetAgentPanel: () => void;
  onAgentSpawnToolStarted: (toolCallId: string, name: string, input: unknown) => void;
  onAgentSpawnToolCompleted: (toolCallId: string, toolName: string, mergedOutput: string) => void;
  applyAgentStreamEvent: (norm: NormalizedStreamEvent, originThreadId?: string) => boolean;
  showApprovalIfOwned: (desktopHost: boolean, payload: ApprovalState) => void;
  /** Called after each tool finishes (office deliverable hook, etc.). */
  onToolCompleted?: (toolName: string, success: boolean, output: string) => void;
  cancelCleanupRef: MutableRefObject<(() => void) | null>;
  userStopRequestedRef: MutableRefObject<boolean>;
  handleCancelStream: () => void;
  streamingRef: MutableRefObject<boolean>;
  /** When user-data or workspace volume is critically low. */
  storagePauseTurns: boolean;
  /**
   * Per-thread stream context registry (required for multi-session parallel streaming).
   */
  streamRegistry: StreamContextRegistry;
  bindThreadSession?: (threadId: string, sessionId: string | null | undefined) => void;
  /** Navigate to a persisted session (background turn complete toast action). */
  onNavigateToSession?: (sessionId: string) => void;
};

export type UseTurnSendResult = {
  handleSend: (
    outbound: ComposerOutboundMessage,
    sendOptions?: { editFromMessageId?: string },
  ) => void;
  resetTurnPersistState: () => void;
};

export function useTurnSend(params: UseTurnSendParams): UseTurnSendResult {
  const {
    t,
    runtimeConn,
    streaming,
    resumedThreadId,
    resumedThreadIdRef,
    runMode,
    autoApprove,
    routeIntent,
    selectedModel,
    selectedWorkspace,
    useWorktree,
    taskTypePreference,
    modelParams,
    desktopHost,
    streamControllersRef,
    pendingSendKeyRef,
    setPendingComposerStream,
    setMessages,
    setResumedThreadId,
    setActiveSessionId,
    setRuntimeSessionEstablished,
    setLastTurnOutputTokens,
    setLastCacheHitPercent,
    setLhtChip,
    activeSessionIdRef,
    sessionUiCacheRef,
    refreshSessions,
    refreshThreadContext,
    applyThreadContextSnapshot,
    applyContextUsageBreakdown,
    notifyRuntimeTransient,
    resetAgentPanel,
    onAgentSpawnToolStarted,
    onAgentSpawnToolCompleted,
    applyAgentStreamEvent,
    showApprovalIfOwned,
    onToolCompleted,
    cancelCleanupRef,
    userStopRequestedRef,
    handleCancelStream,
    streamingRef,
    storagePauseTurns,
    streamRegistry,
    bindThreadSession,
    onNavigateToSession,
  } = params;

  const lastPersistedTurnRef = useRef('');

  const { shouldSkipFinishOnAbort, clearDetachedState } = useTurnStreamRecovery({
    t,
    desktopHost,
    runtimeConn,
    streamingRef,
    resumedThreadIdRef,
    streamControllersRef,
    setMessages,
    setPendingComposerStream,
    handleCancelStream,
    notifyRuntimeTransient,
    refreshThreadContext,
    streamRegistry,
  });

  useEffect(() => {
    cancelCleanupRef.current = clearDetachedState;
    return () => {
      cancelCleanupRef.current = null;
    };
  }, [cancelCleanupRef, clearDetachedState]);

  // Multi-session P0.6 hardening: sync the panel channel's active-thread filter
  // whenever the registry's active thread changes. This lets `dispatchPanel*`
  // drop panel events originating from a non-active thread (defensive guard
  // against future call sites that forget the `isBackground` early-return).
  useEffect(() => {
    setPanelActiveThreadId(streamRegistry.activeThreadId ?? resumedThreadId ?? null);
    return () => {
      // Reset on teardown so the module-level filter does not retain a stale id.
      setPanelActiveThreadId(null);
    };
  }, [streamRegistry.activeThreadId, resumedThreadId, streamRegistry.version]);

  const resetTurnPersistState = useCallback(() => {
    lastPersistedTurnRef.current = '';
  }, []);

  const handleSend = useCallback(
    (
      outbound: ComposerOutboundMessage,
      sendOptions?: { editFromMessageId?: string },
    ) => {
      if (!outbound.apiPrompt.trim() || streaming) return;
      if (storagePauseTurns) {
        notifyRuntimeTransient(t('storage.sendBlocked'));
        return;
      }

      const concurrentStreams = getActiveThreadIdsFromStore().size;
      if (concurrentStreams >= MAX_CONCURRENT_STREAMING_LIMIT) {
        toast.warning(t('composer.concurrentStreamsLimit'));
        return;
      }
      if (concurrentStreams >= MAX_CONCURRENT_STREAMING_WARN) {
        toast.warning(t('composer.concurrentStreamsWarn'));
      }

      userStopRequestedRef.current = false;
      setPendingComposerStream(true);

      // Multi-session cross-talk fix: capture the session id at send time.
      // All persist / migrate / context-bind operations within this send's
      // lifecycle use this closure value, NOT `activeSessionIdRef.current`,
      // so that if the user switches to session B between A's send and A's
      // `turn_started`, A's draft/messages are not migrated to B's session.
      const ownerSessionId = activeSessionIdRef.current;

      const syncResolvedThread =
        resolveThreadIdForSend(
          streamRegistry,
          resumedThreadIdRef.current,
          ownerSessionId,
        ) ||
        null;
      if (syncResolvedThread && syncResolvedThread !== resumedThreadIdRef.current) {
        resumedThreadIdRef.current = syncResolvedThread;
        setResumedThreadId(syncResolvedThread);
      }

      const knownThreadAtSend = syncResolvedThread ?? resumedThreadIdRef.current;
      if (knownThreadAtSend) {
        streamRegistry.ensureContext(knownThreadAtSend, ownerSessionId);
        bindThreadSession?.(knownThreadAtSend, ownerSessionId);
      }
      // Multi-session cross-talk fix: use a per-send unique key for the
      // AbortController when the real threadId is not yet known (brand-new
      // session). This replaces the shared `__pending__` key so that one send
      // completing cannot delete/abort another concurrent pending send's
      // controller. The key is recorded in `pendingSendKeyRef` so that
      // `handleCancelStream` (Esc) can find the right controller.
      const streamKey = resumedThreadIdRef.current ?? nextSendKey();
      pendingSendKeyRef.current = streamKey;
      streamControllersRef.current.get(streamKey)?.abort();
      const controller = new AbortController();
      streamControllersRef.current.set(streamKey, controller);
      const signal = controller.signal;

      // Multi-session P0.3: the thread this turn belongs to. Known up-front
      // when resuming an existing thread; for new-thread `postStreamTurn` it
      // is resolved from `turn_started`. Used by `applyNorm` to route events
      // into the background context when this thread is not the active view.
      let ownerThreadId: string | null = knownThreadAtSend?.trim() || null;
      /** True until this consumer's `turn_started` — keeps new-session sends on the active SSE path. */
      let pendingSend = true;

      const userMsg: TurnChatMessage = {
        id: nextId(),
        role: 'user',
        content: outbound.displayContent,
      };
      setMessages((prev) => {
        const editId = sendOptions?.editFromMessageId;
        const base =
          editId != null
            ? (() => {
                const idx = prev.findIndex((m) => m.id === editId);
                return idx >= 0 ? prev.slice(0, idx) : prev;
              })()
            : prev;
        return [...base, userMsg];
      });

      const streamTarget = { assistantId: nextId() };
      const assistantMsg: TurnChatMessage = {
        id: streamTarget.assistantId,
        role: 'assistant',
        content: '',
        blocks: [],
        isStreaming: true,
      };
      setMessages((prev) => [...prev, assistantMsg]);

      setRuntimeSessionEstablished(true);
      setLhtChip(null);
      resetAgentPanel();
      toast.dismissAll();
      void (async () => {
        let toolProgressPending = '';
        let toolProgressRaf: number | null = null;
        let lastEventSeq = 0;

        const ctx = {
          currentToolId: { current: null as string | null },
        };
        let timelineState = createEmptyTimelineState();

        const applyTimelineNorm = (norm: NormalizedStreamEvent, finalize = false) => {
          setMessages((prev) => {
            const targetId = resolveStreamTargetId(prev, streamTarget);
            const result = applyStreamEventToMessages(prev, timelineState, norm, {
              streamTargetId: targetId,
              currentToolId: ctx.currentToolId.current,
              finalize,
            });
            timelineState = result.timelineState;
            const tid = ownerThreadId || deliveryThreadId;
            if (tid && streamRegistry.getContext(tid)) {
              streamRegistry.patchContext(tid, { timelineState });
            }
            return result.messages;
          });
        };

        const flushToolProgressToState = () => {
          const chunk = toolProgressPending;
          if (!chunk) return;
          toolProgressPending = '';
          applyTimelineNorm({ kind: 'tool_progress', output: chunk });
        };

        const scheduleToolProgressFlush = () => {
          if (toolProgressRaf != null) return;
          toolProgressRaf = requestAnimationFrame(() => {
            toolProgressRaf = null;
            flushToolProgressToState();
          });
        };

        const markThreadStreamIdle = (threadId: string | null | undefined) => {
          const tid = threadId?.trim();
          if (!tid) return;
          const knownSessionId =
            streamRegistry.getContext(tid)?.sessionId ?? activeSessionIdRef.current;
          streamRegistry.ensureContext(tid, knownSessionId);
          streamRegistry.patchContext(tid, {
            isStreaming: false,
            pendingApproval: null,
          });
        };

        const resolveStreamThreadId = (): string | null =>
          ownerThreadId?.trim() ||
          readThreadTurn(streamRegistry, resumedThreadIdRef.current).threadId ||
          resumedThreadIdRef.current?.trim() ||
          streamRegistry.activeThreadIdRef.current?.trim() ||
          null;

        const scheduleThreadSessionPersist = (threadId: string) => {
          const tid = threadId.trim();
          if (!tid) return;
          const knownSessionId =
            streamRegistry.getContext(tid)?.sessionId ?? ownerSessionId;
          void (async () => {
            try {
              const res = await persistThreadSessionDeduped(tid, knownSessionId);
              bindThreadSession?.(tid, res.session_id);
              if (streamRegistry.isActiveStreamView(tid)) {
                setActiveSessionId(res.session_id);
                saveStoredActiveSessionId(res.session_id);
              }
              await refreshSessions();
            } catch {
              /* streaming checkpoint / turn-complete will retry */
            }
          })();
        };

        let finished = false;
        let finishPending = false;
        const interruptedLabel = t('composer.turnInterrupted');
        const markInterrupted = () => {
          setMessages((prev) =>
            prev.map((m) => {
              if (m.id !== streamTarget.assistantId) return m;
              const tools = (m.tools ?? []).map((tool) =>
                tool.status === 'running' ? { ...tool, status: 'error' as const } : tool,
              );
              const trimmed = m.content.trim();
              let content = m.content;
              if (!trimmed) {
                content = interruptedLabel;
              } else if (!trimmed.startsWith(`[${interruptedLabel}]`)) {
                content = `[${interruptedLabel}] ${m.content}`;
              }
              return { ...m, tools, content, isStreaming: false };
            }),
          );
        };

        const maybePersistCompletedTurn = () => {
          const completingId = ownerThreadId || resumedThreadIdRef.current || '';
          const { threadId, turnId } = readThreadTurn(streamRegistry, completingId);
          if (!threadId || !turnId || turnId === lastPersistedTurnRef.current) {
            return;
          }
          lastPersistedTurnRef.current = turnId;
          void (async () => {
            try {
              const res = await persistThreadSessionDeduped(threadId, ownerSessionId);
              if (ownerThreadId && !streamRegistry.isActiveStreamView(ownerThreadId)) {
                await refreshSessions();
                return;
              }
              setActiveSessionId(res.session_id);
              saveStoredActiveSessionId(res.session_id);
              bindThreadSession?.(threadId, res.session_id);
              await refreshSessions();
            } catch (e) {
              toast.error(t('banner.persistSessionFailed', { message: (e as Error).message }));
            }
          })();
        };

        const maybePersistBackgroundTurn = () => {
          const tid = ownerThreadId;
          if (!tid) return;
          const ctxTurnId = readThreadTurn(streamRegistry, tid).turnId;
          if (!ctxTurnId || ctxTurnId === lastPersistedTurnRef.current) {
            return;
          }
          lastPersistedTurnRef.current = ctxTurnId;
          void (async () => {
            try {
              const sessionId = streamRegistry.getContext(tid)?.sessionId ?? null;
              const res = await persistThreadSessionDeduped(tid, sessionId);
              if (streamRegistry.isActiveStreamView(tid)) {
                setActiveSessionId(res.session_id);
                saveStoredActiveSessionId(res.session_id);
              }
              bindThreadSession?.(tid, res.session_id);
              await refreshSessions();
            } catch (e) {
              toast.error(t('banner.persistSessionFailed', { message: (e as Error).message }));
            }
          })();
        };

        const completeBackgroundStream = () => {
          if (finished) return;
          finished = true;
          if (!signal.aborted) {
            controller.abort();
          }
          const tid = ownerThreadId;
          if (tid) {
            markThreadStreamIdle(tid);
            streamControllersRef.current.delete(tid);
          }
          // Only clean up THIS send's per-send key — never a shared key that
          // might belong to a concurrent pending send (cross-talk fix).
          streamControllersRef.current.delete(streamKey);
          if (pendingSendKeyRef.current === streamKey) {
            pendingSendKeyRef.current = null;
          }
        };

        const syncTranscriptFromThread = (threadId: string) => {
          void (async () => {
            try {
              const rebuilt = await rebuildMessagesFromThreadEvents(threadId);
              if (rebuilt.length === 0) {
                return;
              }
              setMessages((prev) => {
                const cleared = clearStreamingAssistants(prev);
                const reconciled = mergeThreadTranscript(
                  cleared,
                  rebuilt as TurnChatMessage[],
                );
                if (sessionMessageRichness(reconciled) <= sessionMessageRichness(cleared)) {
                  return prev;
                }
                const sid = activeSessionIdRef.current;
                if (sid) {
                  cacheSessionUiMessages(sessionUiCacheRef.current, sid, reconciled);
                }
                return reconciled;
              });
            } catch {
              /* keep live snapshot */
            }
          })();
        };

        const completeStreamUi = () => {
          if (finished) return;
          finished = true;
          if (toolProgressRaf != null) {
            cancelAnimationFrame(toolProgressRaf);
            toolProgressRaf = null;
          }
          flushToolProgressToState();
          if (!signal.aborted) {
            controller.abort();
          }
          const finishedThreadId = resolveStreamThreadId();
          // Only clean up THIS send's per-send key (cross-talk fix).
          streamControllersRef.current.delete(streamKey);
          if (finishedThreadId) {
            markThreadStreamIdle(finishedThreadId);
            streamControllersRef.current.delete(finishedThreadId);
            clearActiveStreamHandles(streamRegistry, finishedThreadId);
          }
          clearActiveStreamHandles(streamRegistry, streamKey);
          if (pendingSendKeyRef.current === streamKey) {
            pendingSendKeyRef.current = null;
          }
          setPendingComposerStream(false);
          setMessages((prev) => {
            const targetId = resolveStreamTargetId(prev, streamTarget);
            let working = prev;
            if (timelineState.blocks.length > 0) {
              const finalized = applyStreamEventToMessages(prev, timelineState, {
                kind: 'turn_completed',
              }, {
                streamTargetId: targetId,
                finalize: true,
              });
              working = finalized.messages;
              timelineState = finalized.timelineState;
            }
            const next = clearStreamingAssistants(working);
            const sid = activeSessionIdRef.current;
            if (sid) {
              cacheSessionUiMessages(sessionUiCacheRef.current, sid, next);
            }
            return next;
          });
          const tid = finishedThreadId || readThreadTurn(streamRegistry, resumedThreadIdRef.current).threadId;
          if (tid) {
            void refreshThreadContext(tid);
            syncTranscriptFromThread(tid);
          }
          maybePersistCompletedTurn();
        };

        const finishOnce = (options?: FinishOnceOptions) => {
          const terminalEvent = options?.terminal === true;
          if (ownerThreadId && !streamRegistry.isActiveStreamView(ownerThreadId)) {
            if (options?.force || userStopRequestedRef.current) {
              completeBackgroundStream();
              return;
            }
            if (terminalEvent) {
              completeBackgroundStream();
              maybePersistBackgroundTurn();
              return;
            }
            void threadTurnStillActive(
              ownerThreadId,
              streamRegistry.getContext(ownerThreadId)?.threadTurn.turnId || undefined,
            ).then((active) => {
              if (finished) return;
              if (!active) {
                completeBackgroundStream();
                maybePersistBackgroundTurn();
              }
            });
            return;
          }
          if (finished) {
            if (terminalEvent) {
              setPendingComposerStream(false);
              const tid = ownerThreadId || readThreadTurn(streamRegistry, resumedThreadIdRef.current).threadId;
              if (tid) {
                markThreadStreamIdle(tid);
              }
              setMessages((prev) => {
                if (!anyAssistantStreaming(prev)) return prev;
                return clearStreamingAssistants(prev);
              });
            }
            return;
          }
          const forceStop = options?.force === true || userStopRequestedRef.current;
          if (forceStop || terminalEvent) {
            finishPending = false;
            completeStreamUi();
            return;
          }
          if (finishPending) return;
          const { threadId, turnId } = readThreadTurn(
            streamRegistry,
            ownerThreadId || resumedThreadIdRef.current,
          );
          if (!threadId) {
            completeStreamUi();
            return;
          }
          finishPending = true;
          void threadTurnStillActive(threadId, turnId || undefined)
            .then((active) => {
              finishPending = false;
              if (finished) return;
              if (signal.aborted && shouldSkipFinishOnAbort()) {
                return;
              }
              if (active) {
                setPendingComposerStream(true);
                setMessages((prev) => {
                  const lastId = lastAssistantMessageId(prev);
                  const targetId = lastId ?? streamTarget.assistantId;
                  if (lastId && lastId !== streamTarget.assistantId) {
                    streamTarget.assistantId = lastId;
                  }
                  return rebindStreamingAssistant(prev, targetId) as TurnChatMessage[];
                });
                syncRecoveryContext();
                return;
              }
              completeStreamUi();
            })
            .catch(() => {
              finishPending = false;
              if (!finished) completeStreamUi();
            });
        };
        // Use the per-send streamKey as the registry delivery key until
        // `turn_started` resolves the real threadId. This avoids two concurrent
        // new-session sends writing into the same shared `__pending__` context.
        let deliveryThreadId = knownThreadAtSend?.trim() || streamKey;
        writeStreamSession(streamRegistry, deliveryThreadId, { markInterrupted, finishOnce });

        const notifyTurnCompleteIfAway = (host: boolean, completingThreadId: string) => {
          if (!host) return;
          if (loadNotifyMethod() === 'off') return;
          const isActiveView = streamRegistry.isActiveStreamView(completingThreadId);
          const ctx = streamRegistry.getContext(completingThreadId);
          const sessionId = ctx?.sessionId ?? null;
          const sessionLabel =
            sessionId?.slice(0, 8) ?? completingThreadId.slice(0, 8);

          if (!isActiveView) {
            toast.success(
              t('notification.turnCompleteBackground', { session: sessionLabel }),
              {
                tag: `bg-complete-${completingThreadId}`,
                duration: 8000,
                action:
                  sessionId && onNavigateToSession
                    ? {
                        label: t('composer.switchToSession'),
                        onClick: () => onNavigateToSession(sessionId),
                      }
                    : undefined,
              },
            );
          }

          void (async () => {
            try {
              const { getCurrentWindow } = await import('@tauri-apps/api/window');
              const focused = await getCurrentWindow().isFocused();
              if (focused && isActiveView) return;
              const mod = await import('@tauri-apps/plugin-notification');
              let granted = await mod.isPermissionGranted();
              if (!granted) {
                const perm = await mod.requestPermission();
                granted = perm === 'granted';
              }
              if (granted) {
                mod.sendNotification({
                  title: 'Zagens',
                  body: isActiveView
                    ? t('notification.turnComplete')
                    : t('notification.turnCompleteBackground', { session: sessionLabel }),
                });
              }
            } catch {
              /* browser mode or Tauri API unavailable */
            }
          })();
        };

        const applyNorm = (norm: NormalizedStreamEvent) => {
          if (norm.kind === 'thread_status') {
            // Thread streaming status is owned exclusively by the always-on
            // global status channel (`useThreadStatusGlobalStream`). Ignoring it
            // on the per-thread content SSE avoids backlog replay re-activating a
            // completed thread after `idle` (ghost spinner / composer lock), and
            // keeps a single authoritative feed into `threadStatusStore`.
            return;
          }

          // Multi-session P0.3: resolve the owning thread for this event.
          // `turn_started` carries it explicitly; all subsequent events on the
          // same SSE consumer inherit the `ownerThreadId` closure value.
          const eventThreadId =
            norm.kind === 'turn_started'
              ? norm.threadId
              : ownerThreadId ||
                readThreadTurn(streamRegistry, resumedThreadIdRef.current).threadId ||
                resumedThreadIdRef.current ||
                null;
          const activeThreadId = streamRegistry.activeThreadIdRef.current ?? null;
          const isBackground = isBackgroundStreamEvent(
              activeThreadId,
              eventThreadId,
              ownerThreadId,
              pendingSend,
            );

          // Background-turn event routing: state events (panel/approval/turn
          // lifecycle) are recorded into the background context so the UI can
          // show "still running" / "needs approval"; content deltas are skipped
          // — the transcript is rebuilt from backend replay on reattach
          // (P0.4 `rebuildMessagesFromThreadEvents`), avoiding dual-write
          // complexity against the active view's `streamTarget.assistantId`.
          if (isBackground && eventThreadId) {
            switch (norm.kind) {
              case 'turn_started':
                streamRegistry.ensureContext(eventThreadId, ownerSessionId);
                streamRegistry.patchContext(eventThreadId, {
                  threadTurn: { threadId: eventThreadId, turnId: norm.turnId },
                  isStreaming: true,
                  sessionId: ownerSessionId,
                });
                bindThreadSession?.(eventThreadId, ownerSessionId);
                scheduleThreadSessionPersist(eventThreadId);
                break;
              case 'approval_required':
                streamRegistry.patchContext(eventThreadId, {
                  pendingApproval: {
                    toolCallId: norm.id,
                    toolName: norm.toolName,
                    description: norm.description,
                  },
                });
                // Multi-session P0.8: surface background approvals with a
                // persistent toast so the user can switch back to act on it.
                toast.warning(
                  t('composer.bgApprovalRequired', {
                    thread: eventThreadId.slice(0, 8),
                  }),
                  {
                    tag: `bg-approval-${eventThreadId}`,
                    duration: 0,
                  },
                );                break;
              case 'turn_completed':
              case 'done':
                toast.dismissByTag(`bg-approval-${eventThreadId}`);
                finishOnce({ terminal: true });
                if (norm.kind === 'done' || norm.kind === 'turn_completed') {
                  notifyTurnCompleteIfAway(desktopHost, eventThreadId);
                }
                break;
              case 'error':
                toast.dismissByTag(`bg-approval-${eventThreadId}`);
                finishOnce({ terminal: true });
                break;
              case 'panel_checklist':
                streamRegistry.patchContext(eventThreadId, {
                  panelSlice: {
                    ...streamRegistry.getContext(eventThreadId)!.panelSlice,
                    checklist: normalizeChecklistPayload(norm.checklist),
                  },
                });
                break;
              case 'panel_task_graph':
                streamRegistry.patchContext(eventThreadId, {
                  panelSlice: {
                    ...streamRegistry.getContext(eventThreadId)!.panelSlice,
                    taskGraph: norm.task_graph as HarnessTaskGraph,
                  },
                });
                break;
              case 'panel_context': {
                const panelCtx = norm.context as ThreadContextSnapshot;
                if (panelCtx && typeof panelCtx.estimated_input_tokens === 'number') {
                  streamRegistry.patchContext(eventThreadId, {
                    panelSlice: {
                      ...streamRegistry.getContext(eventThreadId)!.panelSlice,
                      context: panelCtx,
                    },
                  });
                }
                break;
              }
              case 'context_usage': {
                const usage = normalizeContextUsageBreakdown(norm.usage);
                if (usage) {
                  streamRegistry.patchContext(eventThreadId, {
                    panelSlice: {
                      ...streamRegistry.getContext(eventThreadId)!.panelSlice,
                      contextUsage: usage,
                    },
                  });
                }
                break;
              }
              case 'panel_scratchpad': {
                const raw = norm.scratchpad;
                if (raw && typeof raw === 'object' && 'run_id' in (raw as Record<string, unknown>)) {
                  streamRegistry.patchContext(eventThreadId, {
                    panelSlice: {
                      ...streamRegistry.getContext(eventThreadId)!.panelSlice,
                      scratchpad: raw as ScratchpadStatus,
                    },
                  });
                }
                break;
              }
              case 'status': {
                const chip = parseLhtStatusMessage(norm.message);
                if (chip) {
                  streamRegistry.patchContext(eventThreadId, {
                    panelSlice: {
                      ...streamRegistry.getContext(eventThreadId)!.panelSlice,
                      lhtChip: chip,
                    },
                  });
                }
                break;
              }
              default:
                // thinking_delta / message_delta / tool_* / agent_* / craft_* /
                // harness_cycle_advanced: skipped for background turns.
                break;
            }
            return;
          }

          switch (norm.kind) {
            case 'turn_started':
              timelineState = createEmptyTimelineState();
              ownerThreadId = norm.threadId;
              pendingSend = false;
              if (norm.threadId) {
                deliveryThreadId = norm.threadId;
                streamRegistry.migrateDraftToThread(
                  ownerSessionId,
                  norm.threadId,
                );
                streamRegistry.ensureContext(norm.threadId, ownerSessionId);
                writeThreadTurn(streamRegistry, norm.threadId, norm.turnId);
                streamRegistry.patchContext(norm.threadId, {
                  sessionId: ownerSessionId,
                  isStreaming: true,
                  timelineState: createEmptyTimelineState(),
                });
                writeStreamSession(streamRegistry, norm.threadId, { markInterrupted, finishOnce });
                writeLiveDeliver(streamRegistry, norm.threadId, onSseEvent);
                clearActiveStreamHandles(streamRegistry, streamKey);
                bindThreadSession?.(norm.threadId, ownerSessionId);
              }
              syncRecoveryContext();
              if (norm.threadId) {
                scheduleThreadSessionPersist(norm.threadId);
                streamRegistry.setActiveThreadId(norm.threadId);
                resumedThreadIdRef.current = norm.threadId;
                setResumedThreadId(norm.threadId);
                void registerWindowThread(norm.threadId);
                setPendingComposerStream(false);
                // Migrate the controller from the per-send key to the real
                // threadId so that subsequent cancel/abort resolves correctly.
                // Only this send's controller is moved — no shared `__pending__`.
                const pending = streamControllersRef.current.get(streamKey);
                if (pending) {
                  streamControllersRef.current.delete(streamKey);
                  streamControllersRef.current.set(norm.threadId, pending);
                }
                pendingSendKeyRef.current = null;
              }
              break;
            case 'thinking_delta':
              applyTimelineNorm(norm);
              break;
            case 'message_delta':
              applyTimelineNorm(norm);
              break;
            case 'message_segment':
              applyTimelineNorm(norm);
              break;
            case 'tool_started': {
              ctx.currentToolId.current = norm.id;
              onAgentSpawnToolStarted(norm.id, norm.name, norm.input);
              applyTimelineNorm(norm);
              break;
            }
            case 'tool_progress':
              toolProgressPending += norm.output;
              scheduleToolProgressFlush();
              break;
            case 'tool_completed': {
              if (toolProgressRaf != null) {
                cancelAnimationFrame(toolProgressRaf);
                toolProgressRaf = null;
              }
              flushToolProgressToState();
              const outStr = capToolOutputForDisplay(toolOutputString(norm.output));
              applyTimelineNorm({
                kind: 'tool_completed',
                id: norm.id,
                success: norm.success,
                output: outStr,
              });
              setMessages((prev) => {
                const targetId = resolveStreamTargetId(prev, streamTarget);
                const assistant = prev.find((m) => m.id === targetId);
                const tool = assistant?.tools?.find((t) => t.id === norm.id)
                  ?? assistant?.tools?.slice().reverse().find((t) => t.status === 'running');
                if (tool) {
                  const merged = capToolOutputForDisplay(
                    mergeStreamingToolOutput(tool.output ?? '', outStr || ''),
                  );
                  onAgentSpawnToolCompleted(norm.id, tool.name, merged);
                  onToolCompleted?.(tool.name, norm.success, merged);
                }
                return prev;
              });
              if (ctx.currentToolId.current === norm.id) {
                ctx.currentToolId.current = null;
              }
              break;
            }
            case 'approval_required':
              showApprovalIfOwned(desktopHost, {
                toolCallId: norm.id,
                toolName: norm.toolName,
                description: norm.description,
              });
              break;
            case 'turn_completed':
              finishOnce({ terminal: true });
              notifyTurnCompleteIfAway(
                desktopHost,
                ownerThreadId || readThreadTurn(streamRegistry, resumedThreadIdRef.current).threadId,
              );
              if (norm.usage?.output_tokens != null && norm.usage.output_tokens > 0) {
                setLastTurnOutputTokens(norm.usage.output_tokens);
              }
              if (norm.usage) {
                const pct = turnCacheHitPercent(norm.usage);
                setLastCacheHitPercent(pct);
              }
              break;
            case 'done':
              finishOnce({ terminal: true });
              notifyTurnCompleteIfAway(
                desktopHost,
                ownerThreadId || readThreadTurn(streamRegistry, resumedThreadIdRef.current).threadId,
              );
              break;
            case 'error':
              finishOnce({ terminal: true });
              setMessages((prev) => {
                const next = clearStreamingAssistants(prev);
                const lastId = lastAssistantMessageId(prev) ?? streamTarget.assistantId;
                return next.map((m) =>
                  m.id === lastId && m.role === 'assistant'
                    ? { ...m, content: m.content || `Error: ${norm.message}` }
                    : m,
                );
              });
              toast.error(norm.message ? norm.message : t('banner.streamError'));
              break;
            case 'agent_spawned':
            case 'agent_progress':
            case 'agent_completed':
            case 'agent_list':
              applyAgentStreamEvent(norm, ownerThreadId || eventThreadId || undefined);
              break;
            case 'panel_scratchpad': {
              const raw = norm.scratchpad;
              if (raw && typeof raw === 'object' && 'run_id' in (raw as Record<string, unknown>)) {
                // P0.6 hardening: pass originThreadId so panelChannel can drop
                // the event if it is not from the active view's thread.
                dispatchPanelScratchpad(raw as ScratchpadStatus, ownerThreadId || eventThreadId || undefined);
              }
              break;
            }
            case 'panel_checklist':
              dispatchPanelChecklist(normalizeChecklistPayload(norm.checklist), ownerThreadId || eventThreadId || undefined);
              break;
            case 'panel_task_graph':
              dispatchPanelTaskGraph(norm.task_graph as HarnessTaskGraph, ownerThreadId || eventThreadId || undefined);
              break;
            case 'harness_cycle_advanced':
              dispatchHarnessCycleAdvanced({ from: norm.from, to: norm.to }, ownerThreadId || eventThreadId || undefined);
              break;
            case 'panel_context': {
              const panelCtx = norm.context as ThreadContextSnapshot;
              const tid = resumedThreadIdRef.current;
              if (tid && panelCtx && typeof panelCtx.estimated_input_tokens === 'number') {
                applyThreadContextSnapshot(tid, panelCtx);
                dispatchPanelContext(panelCtx, ownerThreadId || eventThreadId || undefined);
              }
              break;
            }
            case 'context_usage': {
              const usage = normalizeContextUsageBreakdown(norm.usage);
              const tid = resumedThreadIdRef.current;
              if (tid && usage) {
                applyContextUsageBreakdown(tid, usage);
                dispatchPanelContextUsage(usage, ownerThreadId || eventThreadId || undefined);
              }
              break;
            }
            case 'craft_verdict':
            case 'craft_board_updated':
              notifyCraftBlackboardChanged();
              break;
            case 'status': {
              const chip = parseLhtStatusMessage(norm.message);
              if (chip) {
                setLhtChip(chip);
              }
              break;
            }
            default:
              break;
          }
        };

        const deliverSseEvent = (ev: SseTurnEvent & { seq?: number }, filter?: { turnId: string }) => {
          if (finished || signal.aborted) return;
          const seq = sseEventSeq(ev);
          if (seq != null) {
            if (seq <= lastEventSeq) {
              return;
            }
            lastEventSeq = seq;
          }
          const norm = normalizeDesktopStreamEvent(ev, filter);
          if (norm) {
            applyNorm(norm);
          }
        };

        const onSseEvent = (ev: SseTurnEvent, filter?: { turnId: string }) => {
          if (!desktopHost) {
            deliverSseEvent(ev, filter);
            return;
          }
          const tid =
            resumedThreadId ||
            readThreadTurn(streamRegistry, resumedThreadIdRef.current).threadId ||
            threadIdFromSseEvent(ev);
          if (!tid) {
            deliverSseEvent(ev, filter);
            return;
          }
          filterThreadStreamEvents(tid, () => deliverSseEvent(ev, filter))(ev);
        };
        writeLiveDeliver(streamRegistry, deliveryThreadId, onSseEvent);

        const syncRecoveryContext = () => {
          const tid = ownerThreadId || deliveryThreadId;
          const { threadId, turnId } = readThreadTurn(streamRegistry, tid);
          if (!threadId || !turnId) return;
          writeRecoveryCtx(streamRegistry, threadId, {
            assistantId: streamTarget.assistantId,
            threadId,
            turnId,
            deliverSseEvent: (ev, filter) => onSseEvent(ev, filter),
            finishOnce,
          });
        };

        const handleHttpError = (err: Error & { status?: number }) => {
          const msg = err.message || String(err);
          const status = err.status;
          if (status === 401) {
            notifyRuntimeTransient(t('banner.unauthorizedBearer'));
          } else if (/api\s*key|DEEPSEEK_API_KEY|401|unauthorized/i.test(msg)) {
            notifyRuntimeTransient(t('banner.missingApiKey'));
          } else if (
            useWorktree &&
            /worktree|git repository|use_worktree|not a git/i.test(msg)
          ) {
            toast.error(t('composer.worktreeFailed', { message: msg }));
          }
          setMessages((prev) => {
            const next = clearStreamingAssistants(prev);
            const lastId = lastAssistantMessageId(prev) ?? streamTarget.assistantId;
            return next.map((m) =>
              m.id === lastId && m.role === 'assistant'
                ? { ...m, content: m.content || `Error: ${msg}` }
                : m,
            );
          });
          finishOnce({ force: true });
        };

        const streamOpts = streamFlagsForRunMode(runMode, autoApprove);
        const routeIntentApi = resolveRouteIntentForApi(routeIntent, runMode);

        const repairStaleStreamingThreads = async () => {
          const ids = getActiveThreadIdsFromStore();
          if (!ids.size) return;
          await Promise.all(
            [...ids].map(async (tid) => {
              try {
                const ctx = streamRegistry.getContext(tid);
                const turnId = ctx?.threadTurn.turnId || '';
                const active = await threadTurnStillActive(tid, turnId || undefined);
                if (!active) {
                  markThreadStreamIdle(tid);
                }
              } catch {
                markThreadStreamIdle(tid);
              }
            }),
          );
        };

        await repairStaleStreamingThreads();

        let sendThreadId =
          resolveThreadIdForSend(
            streamRegistry,
            resumedThreadIdRef.current,
            ownerSessionId,
          ) ?? '';

        if (!sendThreadId && ownerSessionId?.trim()) {
          try {
            const resumed = await resumeSessionThread(ownerSessionId.trim());
            sendThreadId = resumed.thread_id?.trim() ?? '';
            if (sendThreadId) {
              resumedThreadIdRef.current = sendThreadId;
              setResumedThreadId(sendThreadId);
              bindThreadSession?.(sendThreadId, ownerSessionId);
              ownerThreadId = sendThreadId;
            }
          } catch {
            /* orphan session — fall through to postStreamTurn */
          }
        } else if (sendThreadId && sendThreadId !== ownerThreadId) {
          resumedThreadIdRef.current = sendThreadId;
          setResumedThreadId(sendThreadId);
          ownerThreadId = sendThreadId;
        }

        try {
          if (sendThreadId) {
            const detail = await getThreadDetail(sendThreadId);
            if (signal.aborted) {
              finishOnce();
              return;
            }
            const sinceSeq = detail.latest_seq ?? 0;
            lastEventSeq = sinceSeq;
            const turnBody = {
              model: selectedModel,
              mode: streamOpts.mode,
              allow_shell: streamOpts.allow_shell,
              trust_mode: streamOpts.trust_mode,
              auto_approve: streamOpts.auto_approve,
              ...(routeIntentApi != null ? { route_intent: routeIntentApi } : {}),
              ...modelSamplingForApi(modelParams, selectedModel),
            };
            const { turn } = sendOptions?.editFromMessageId
              ? await editLastThreadTurn(sendThreadId, {
                  content: outbound.apiPrompt,
                  ...turnBody,
                })
              : await startThreadTurn(sendThreadId, {
                  prompt: outbound.apiPrompt,
                  task_type: taskTypePreference,
                  ...turnBody,
                });
            if (signal.aborted) {
              finishOnce();
              return;
            }
            const turnId = turn.id;
            deliveryThreadId = sendThreadId;
            writeThreadTurn(streamRegistry, sendThreadId, turnId);
            writeStreamSession(streamRegistry, sendThreadId, { markInterrupted, finishOnce });
            writeLiveDeliver(streamRegistry, sendThreadId, onSseEvent);
            syncRecoveryContext();

            await pollThreadTurnEvents(
              sendThreadId,
              sinceSeq,
              (ev) => onSseEvent(ev, { turnId }),
              { signal, turnId },
            );
            if (!shouldSkipFinishOnAbort()) {
              finishOnce();
            }
          } else {
            await postStreamTurn(
              {
                prompt: outbound.apiPrompt,
                workspace: selectedWorkspace,
                mode: streamOpts.mode,
                model: selectedModel,
                allow_shell: streamOpts.allow_shell,
                trust_mode: streamOpts.trust_mode,
                auto_approve: streamOpts.auto_approve,
                ...(routeIntentApi != null ? { route_intent: routeIntentApi } : {}),
                task_type: taskTypePreference,
                ...(useWorktree ? { use_worktree: true } : {}),
                ...modelSamplingForApi(modelParams, selectedModel),
              },
              (ev) => onSseEvent(ev),
              () => {
                if (!shouldSkipFinishOnAbort()) {
                  finishOnce();
                }
              },
              (err) => handleHttpError(err as Error & { status?: number }),
              { signal },
            );
          }
        } catch (e) {
          if ((e as Error).name === 'AbortError') {
            if (shouldSkipFinishOnAbort()) {
              return;
            }
            finishOnce();
            return;
          }
          // Composer desync recovery: the backend already has a turn running for
          // this thread (our local stream had closed early and unlocked the box).
          // Reconnect to that turn instead of surfacing a raw error, and keep the
          // composer locked until it actually ends.
          const emsg = (e as Error).message || '';
          if (sendThreadId && /active turn/i.test(emsg)) {
            const recovered = await (async () => {
              try {
                const detail = await getThreadDetail(sendThreadId);
                const activeTurnId = detail.thread.latest_turn_id ?? undefined;
                if (!(await threadTurnStillActive(sendThreadId, activeTurnId))) {
                  return false;
                }
                // The just-typed prompt was rejected — drop optimistic bubbles, rebind live stream
                // to the last assistant row so thinking/tools/output SSE deltas apply again.
                setMessages((prev) => {
                  const filtered = prev.filter(
                    (m) => m.id !== userMsg.id && m.id !== streamTarget.assistantId,
                  );
                  const lastId = lastAssistantMessageId(filtered);
                  if (lastId) {
                    streamTarget.assistantId = lastId;
                    return rebindStreamingAssistant(filtered, lastId) as TurnChatMessage[];
                  }
                  const fallbackId = nextId();
                  streamTarget.assistantId = fallbackId;
                  return [
                    ...filtered,
                    {
                      id: fallbackId,
                      role: 'assistant',
                      content: '',
                      isStreaming: true,
                    },
                  ];
                });
                deliveryThreadId = sendThreadId;
                writeThreadTurn(streamRegistry, sendThreadId, activeTurnId ?? '');
                writeStreamSession(streamRegistry, sendThreadId, { markInterrupted, finishOnce });
                writeLiveDeliver(streamRegistry, sendThreadId, onSseEvent);
                resumedThreadIdRef.current = sendThreadId;
                setResumedThreadId(sendThreadId);
                setPendingComposerStream(true);
                syncRecoveryContext();
                toast.warning(t('composer.turnStillRunning'));
                lastEventSeq = detail.latest_seq ?? 0;
                await pollThreadTurnEvents(
                  sendThreadId,
                  detail.latest_seq ?? 0,
                  (ev) => onSseEvent(ev, activeTurnId ? { turnId: activeTurnId } : undefined),
                  { signal, turnId: activeTurnId },
                );
                if (!shouldSkipFinishOnAbort()) {
                  finishOnce();
                }
                return true;
              } catch {
                return false;
              }
            })();
            if (recovered) return;
          }
          handleHttpError(e as Error & { status?: number });
        }
      })();
    },
    [
      shouldSkipFinishOnAbort,
      storagePauseTurns,
      streaming,
      runtimeConn,
      resumedThreadId,
      resumedThreadIdRef,
      runMode,
      autoApprove,
      routeIntent,
      selectedModel,
      selectedWorkspace,
      useWorktree,
      taskTypePreference,
      modelParams,
      desktopHost,
      streamControllersRef,
      setPendingComposerStream,
      setMessages,
      setResumedThreadId,
      setActiveSessionId,
      setRuntimeSessionEstablished,
      setLastTurnOutputTokens,
      setLastCacheHitPercent,
      activeSessionIdRef,
      sessionUiCacheRef,
      refreshSessions,
      refreshThreadContext,
      applyThreadContextSnapshot,
      notifyRuntimeTransient,
      resetAgentPanel,
      onAgentSpawnToolStarted,
      onAgentSpawnToolCompleted,
      applyAgentStreamEvent,
      showApprovalIfOwned,
      userStopRequestedRef,
      t,
      streamRegistry,
      bindThreadSession,
    ],
  );

  return { handleSend, resetTurnPersistState };
}
