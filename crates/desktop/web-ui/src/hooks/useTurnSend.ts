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
  persistThreadSession,
  startThreadTurn,
  threadIdFromSseEvent,
  threadTurnStillActive,
  type RuntimeConnectionState,
  type SseTurnEvent,
} from '../api/client';
import type { ComposerOutboundMessage } from '../components/Composer';
import { normalizeDesktopStreamEvent, type NormalizedStreamEvent } from '../api/streamNormalize';
import { notifyCraftBlackboardChanged } from '../lib/craftBlackboard';
import { loadNotifyMethod } from '../lib/appPreferences';
import {
  appendCappedToolOutput,
  capToolOutputForDisplay,
  mergeStreamingToolOutput,
  stringifyToolInput,
  toolOutputString,
} from '../lib/chat/toolOutput';
import {
  cacheSessionUiMessages,
  type CachedUiMessage,
} from '../lib/chat/sessionUiCache';
import type { ThreadContextSnapshot } from '../lib/contextUsage';
import type { ModelParams } from '../components/ModelParamsDialog';
import { modelSamplingForApi } from '../lib/modelParams';
import {
  dispatchPanelChecklist,
  dispatchPanelContext,
  dispatchPanelScratchpad,
  dispatchPanelTaskGraph,
  dispatchHarnessCycleAdvanced,
  normalizeChecklistPayload,
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
import type { StreamSessionControl } from './useTurnStream';
import type { ScratchpadStatus } from '../api/client';
import { saveStoredActiveSessionId } from '../lib/windowBridge';
import { turnCacheHitPercent } from '../lib/cacheUsage';
import { parseLhtStatusMessage, type LhtChipState } from '../lib/lhtChip';
import {
  lastAssistantMessageId,
  rebindStreamingAssistant,
} from '../lib/chat/activeTurnStreamUi';
import {
  useTurnStreamRecovery,
  type StreamRecoveryContext,
} from './useTurnStreamRecovery';

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
  isStreaming?: boolean;
};

let msgId = 0;
function nextId() {
  return `msg-${++msgId}`;
}

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
  taskTypePreference: DesktopTaskTypePreference;
  modelParams: ModelParams;
  desktopHost: boolean;
  streamControllersRef: MutableRefObject<Map<string, AbortController>>;
  threadTurnRef: MutableRefObject<{ threadId: string; turnId: string }>;
  streamSessionRef: MutableRefObject<StreamSessionControl | null>;
  setStreamingThreadIds: Dispatch<SetStateAction<Set<string>>>;
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
  notifyRuntimeTransient: (message: string) => void;
  resetAgentPanel: () => void;
  onAgentSpawnToolStarted: (toolCallId: string, name: string, input: unknown) => void;
  onAgentSpawnToolCompleted: (toolCallId: string, toolName: string, mergedOutput: string) => void;
  applyAgentStreamEvent: (norm: NormalizedStreamEvent) => boolean;
  showApprovalIfOwned: (desktopHost: boolean, payload: ApprovalState) => void;
  /** Called after each tool finishes (office deliverable hook, etc.). */
  onToolCompleted?: (toolName: string, success: boolean, output: string) => void;
  cancelCleanupRef: MutableRefObject<(() => void) | null>;
  userStopRequestedRef: MutableRefObject<boolean>;
  handleCancelStream: () => void;
  streamingRef: MutableRefObject<boolean>;
  /** When user-data or workspace volume is critically low. */
  storagePauseTurns: boolean;
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
    taskTypePreference,
    modelParams,
    desktopHost,
    streamControllersRef,
    threadTurnRef,
    streamSessionRef,
    setStreamingThreadIds,
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
  } = params;

  const lastPersistedTurnRef = useRef('');
  const toolProgressPendingRef = useRef('');
  const toolProgressRafRef = useRef<number | null>(null);
  const streamRecoveryContextRef = useRef<StreamRecoveryContext | null>(null);
  const liveStreamDeliverRef = useRef<
    ((ev: SseTurnEvent, filter?: { turnId: string }) => void) | null
  >(null);

  const { shouldSkipFinishOnAbort, clearDetachedState } = useTurnStreamRecovery({
    t,
    desktopHost,
    runtimeConn,
    streamingRef,
    resumedThreadIdRef,
    threadTurnRef,
    streamControllersRef,
    streamSessionRef,
    streamRecoveryContextRef,
    liveStreamDeliverRef,
    setMessages,
    setStreamingThreadIds,
    setPendingComposerStream,
    handleCancelStream,
    notifyRuntimeTransient,
    refreshThreadContext,
  });

  useEffect(() => {
    cancelCleanupRef.current = clearDetachedState;
    return () => {
      cancelCleanupRef.current = null;
    };
  }, [cancelCleanupRef, clearDetachedState]);

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

      userStopRequestedRef.current = false;
      setPendingComposerStream(true);
      const streamKey = resumedThreadIdRef.current ?? '__pending__';
      streamControllersRef.current.get(streamKey)?.abort();
      const controller = new AbortController();
      streamControllersRef.current.set(streamKey, controller);
      const signal = controller.signal;

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
        isStreaming: true,
      };
      setMessages((prev) => [...prev, assistantMsg]);

      setRuntimeSessionEstablished(true);
      setLhtChip(null);
      resetAgentPanel();
      toast.dismissAll();
      toolProgressPendingRef.current = '';
      if (toolProgressRafRef.current != null) {
        cancelAnimationFrame(toolProgressRafRef.current);
        toolProgressRafRef.current = null;
      }
      void (async () => {
        const ctx = {
          currentToolId: { current: null as string | null },
        };

        const flushToolProgressToState = () => {
          const chunk = toolProgressPendingRef.current;
          if (!chunk) return;
          toolProgressPendingRef.current = '';
          setMessages((prev) =>
            prev.map((m) => {
              if (m.id !== streamTarget.assistantId) return m;
              const tools = [...(m.tools ?? [])];
              let idx = -1;
              if (ctx.currentToolId.current) {
                idx = tools.findIndex((tool) => tool.id === ctx.currentToolId.current);
              }
              if (idx < 0) {
                for (let i = tools.length - 1; i >= 0; i--) {
                  if (tools[i].status === 'running') {
                    idx = i;
                    break;
                  }
                }
              }
              if (idx < 0) return m;
              const tool = tools[idx];
              tools[idx] = {
                ...tool,
                output: appendCappedToolOutput(tool.output ?? '', chunk),
              };
              return { ...m, tools };
            }),
          );
        };

        const scheduleToolProgressFlush = () => {
          if (toolProgressRafRef.current != null) return;
          toolProgressRafRef.current = requestAnimationFrame(() => {
            toolProgressRafRef.current = null;
            flushToolProgressToState();
          });
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
          const { threadId, turnId } = threadTurnRef.current;
          if (!threadId || !turnId || turnId === lastPersistedTurnRef.current) {
            return;
          }
          lastPersistedTurnRef.current = turnId;
          void (async () => {
            try {
              const res = await persistThreadSession(threadId, activeSessionIdRef.current);
              setActiveSessionId(res.session_id);
              saveStoredActiveSessionId(res.session_id);
              await refreshSessions();
            } catch (e) {
              toast.error(t('banner.persistSessionFailed', { message: (e as Error).message }));
            }
          })();
        };

        const completeStreamUi = () => {
          if (finished) return;
          finished = true;
          userStopRequestedRef.current = false;
          liveStreamDeliverRef.current = null;
          streamSessionRef.current = null;
          streamRecoveryContextRef.current = null;
          if (toolProgressRafRef.current != null) {
            cancelAnimationFrame(toolProgressRafRef.current);
            toolProgressRafRef.current = null;
          }
          flushToolProgressToState();
          if (!signal.aborted) {
            controller.abort();
          }
          const finishedThreadId = threadTurnRef.current.threadId;
          streamControllersRef.current.delete('__pending__');
          if (finishedThreadId) {
            setStreamingThreadIds((prev) => {
              const next = new Set(prev);
              next.delete(finishedThreadId);
              return next;
            });
            streamControllersRef.current.delete(finishedThreadId);
          }
          setPendingComposerStream(false);
          setMessages((prev) => {
            const next = prev.map((m) =>
              m.id === streamTarget.assistantId ? { ...m, isStreaming: false } : m,
            );
            const sid = activeSessionIdRef.current;
            if (sid) {
              cacheSessionUiMessages(sessionUiCacheRef.current, sid, next);
            }
            return next;
          });
          const tid = threadTurnRef.current.threadId;
          if (tid) {
            void refreshThreadContext(tid);
          }
          maybePersistCompletedTurn();
        };

        const finishOnce = (options?: { force?: boolean }) => {
          if (finished) return;
          const forceStop = options?.force === true || userStopRequestedRef.current;
          if (forceStop) {
            finishPending = false;
            completeStreamUi();
            return;
          }
          if (finishPending) return;
          const { threadId, turnId } = threadTurnRef.current;
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
                setStreamingThreadIds((prev) => new Set(prev).add(threadId));
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
        streamSessionRef.current = { markInterrupted, finishOnce };

        const notifyTurnCompleteIfAway = (host: boolean) => {
          if (!host) return;
          // Respect the user's notify_method preference; 'off' suppresses all notifications.
          if (loadNotifyMethod() === 'off') return;
          void (async () => {
            try {
              // Use Tauri's isFocused() — more reliable than document.hidden in WebView2 on Windows.
              const { getCurrentWindow } = await import('@tauri-apps/api/window');
              const focused = await getCurrentWindow().isFocused();
              if (focused) return;
              const mod = await import('@tauri-apps/plugin-notification');
              let granted = await mod.isPermissionGranted();
              if (!granted) {
                const perm = await mod.requestPermission();
                granted = perm === 'granted';
              }
              if (granted) {
                mod.sendNotification({ title: 'Zagens', body: t('notification.turnComplete') });
              }
            } catch {
              /* browser mode or Tauri API unavailable */
            }
          })();
        };

        const applyNorm = (norm: NormalizedStreamEvent) => {
          switch (norm.kind) {
            case 'turn_started':
              threadTurnRef.current = {
                threadId: norm.threadId,
                turnId: norm.turnId,
              };
              syncRecoveryContext();
              if (norm.threadId) {
                setResumedThreadId(norm.threadId);
                void registerWindowThread(norm.threadId);
                setStreamingThreadIds((prev) => new Set(prev).add(norm.threadId));
                setPendingComposerStream(false);
                const pending = streamControllersRef.current.get('__pending__');
                if (pending) {
                  streamControllersRef.current.delete('__pending__');
                  streamControllersRef.current.set(norm.threadId, pending);
                }
              }
              break;
            case 'thinking_delta':
              setMessages((prev) =>
                prev.map((m) => {
                  if (m.id !== streamTarget.assistantId) return m;
                  return { ...m, thinking: (m.thinking ?? '') + norm.content };
                }),
              );
              break;
            case 'message_delta':
              setMessages((prev) =>
                prev.map((m) => {
                  if (m.id !== streamTarget.assistantId) return m;
                  return { ...m, content: m.content + norm.content };
                }),
              );
              break;
            case 'tool_started': {
              ctx.currentToolId.current = norm.id;
              onAgentSpawnToolStarted(norm.id, norm.name, norm.input);
              const inputStr = stringifyToolInput(norm.input);
              setMessages((prev) =>
                prev.map((m) => {
                  if (m.id !== streamTarget.assistantId) return m;
                  const tools = [
                    ...(m.tools ?? []),
                    { id: norm.id, name: norm.name, input: inputStr, status: 'running' as const },
                  ];
                  return { ...m, tools };
                }),
              );
              break;
            }
            case 'tool_progress':
              toolProgressPendingRef.current += norm.output;
              scheduleToolProgressFlush();
              break;
            case 'tool_completed': {
              if (toolProgressRafRef.current != null) {
                cancelAnimationFrame(toolProgressRafRef.current);
                toolProgressRafRef.current = null;
              }
              flushToolProgressToState();
              const outStr = capToolOutputForDisplay(toolOutputString(norm.output));
              setMessages((prev) =>
                prev.map((m) => {
                  if (m.id !== streamTarget.assistantId) return m;
                  const tools = [...(m.tools ?? [])];
                  let idx = tools.findIndex((tool) => tool.id === norm.id);
                  if (idx < 0) {
                    for (let i = tools.length - 1; i >= 0; i--) {
                      if (tools[i].status === 'running') {
                        idx = i;
                        break;
                      }
                    }
                  }
                  if (idx < 0) return m;
                  const tool = tools[idx];
                  const prevOut = (tool.output ?? '').trim();
                  const finalOut = outStr.trim();
                  const merged = capToolOutputForDisplay(
                    mergeStreamingToolOutput(prevOut, finalOut || ''),
                  );
                  tools[idx] = {
                    ...tool,
                    output: merged,
                    status: norm.success ? ('done' as const) : ('error' as const),
                  };
                  onAgentSpawnToolCompleted(norm.id, tool.name, merged);
                  onToolCompleted?.(tool.name, norm.success, merged);
                  return { ...m, tools };
                }),
              );
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
              finishOnce();
              notifyTurnCompleteIfAway(desktopHost);
              if (norm.usage?.output_tokens != null && norm.usage.output_tokens > 0) {
                setLastTurnOutputTokens(norm.usage.output_tokens);
              }
              if (norm.usage) {
                const pct = turnCacheHitPercent(norm.usage);
                setLastCacheHitPercent(pct);
              }
              break;
            case 'done':
              finishOnce();
              notifyTurnCompleteIfAway(desktopHost);
              break;
            case 'error':
              finishOnce();
              setMessages((prev) =>
                prev.map((m) =>
                  m.id === streamTarget.assistantId
                    ? { ...m, content: m.content || `Error: ${norm.message}`, isStreaming: false }
                    : m,
                ),
              );
              toast.error(norm.message ? norm.message : t('banner.streamError'));
              break;
            case 'agent_spawned':
            case 'agent_progress':
            case 'agent_completed':
            case 'agent_list':
              applyAgentStreamEvent(norm);
              break;
            case 'panel_scratchpad': {
              const raw = norm.scratchpad;
              if (raw && typeof raw === 'object' && 'run_id' in (raw as Record<string, unknown>)) {
                dispatchPanelScratchpad(raw as ScratchpadStatus);
              }
              break;
            }
            case 'panel_checklist':
              dispatchPanelChecklist(normalizeChecklistPayload(norm.checklist));
              break;
            case 'panel_task_graph':
              dispatchPanelTaskGraph(norm.task_graph as HarnessTaskGraph);
              break;
            case 'harness_cycle_advanced':
              dispatchHarnessCycleAdvanced({ from: norm.from, to: norm.to });
              break;
            case 'panel_context': {
              const panelCtx = norm.context as ThreadContextSnapshot;
              const tid = resumedThreadIdRef.current;
              if (tid && panelCtx && typeof panelCtx.estimated_input_tokens === 'number') {
                applyThreadContextSnapshot(tid, panelCtx);
                dispatchPanelContext(panelCtx);
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

        const deliverSseEvent = (ev: SseTurnEvent, filter?: { turnId: string }) => {
          if (signal.aborted) return;
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
            resumedThreadId || threadTurnRef.current.threadId || threadIdFromSseEvent(ev);
          if (!tid) {
            deliverSseEvent(ev, filter);
            return;
          }
          filterThreadStreamEvents(tid, () => deliverSseEvent(ev, filter))(ev);
        };
        liveStreamDeliverRef.current = onSseEvent;

        const syncRecoveryContext = () => {
          const { threadId, turnId } = threadTurnRef.current;
          if (!threadId || !turnId) return;
          streamRecoveryContextRef.current = {
            assistantId: streamTarget.assistantId,
            threadId,
            turnId,
            deliverSseEvent: (ev, filter) => onSseEvent(ev, filter),
            finishOnce,
          };
        };

        const handleHttpError = (err: Error & { status?: number }) => {
          const msg = err.message || String(err);
          const status = err.status;
          if (status === 401) {
            notifyRuntimeTransient(t('banner.unauthorizedBearer'));
          } else if (/api\s*key|DEEPSEEK_API_KEY|401|unauthorized/i.test(msg)) {
            notifyRuntimeTransient(t('banner.missingApiKey'));
          }
          setMessages((prev) =>
            prev.map((m) =>
              m.id === streamTarget.assistantId
                ? { ...m, content: m.content || `Error: ${msg}`, isStreaming: false }
                : m,
            ),
          );
          finishOnce({ force: true });
        };

        const streamOpts = streamFlagsForRunMode(runMode, autoApprove);
        const routeIntentApi = resolveRouteIntentForApi(routeIntent, runMode);

        try {
          if (resumedThreadId) {
            const detail = await getThreadDetail(resumedThreadId);
            if (signal.aborted) {
              finishOnce();
              return;
            }
            const sinceSeq = detail.latest_seq ?? 0;
            const turnBody = {
              model: selectedModel,
              mode: streamOpts.mode,
              allow_shell: streamOpts.allow_shell,
              trust_mode: streamOpts.trust_mode,
              auto_approve: streamOpts.auto_approve,
              ...(routeIntentApi != null ? { route_intent: routeIntentApi } : {}),
              ...modelSamplingForApi(modelParams),
            };
            const { turn } = sendOptions?.editFromMessageId
              ? await editLastThreadTurn(resumedThreadId, {
                  content: outbound.apiPrompt,
                  ...turnBody,
                })
              : await startThreadTurn(resumedThreadId, {
                  prompt: outbound.apiPrompt,
                  task_type: taskTypePreference,
                  ...turnBody,
                });
            if (signal.aborted) {
              finishOnce();
              return;
            }
            const turnId = turn.id;
            threadTurnRef.current = {
              threadId: resumedThreadId,
              turnId,
            };
            syncRecoveryContext();

            await pollThreadTurnEvents(
              resumedThreadId,
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
                ...modelSamplingForApi(modelParams),
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
          if (resumedThreadId && /active turn/i.test(emsg)) {
            const recovered = await (async () => {
              try {
                const detail = await getThreadDetail(resumedThreadId);
                const activeTurnId = detail.thread.latest_turn_id ?? undefined;
                if (!(await threadTurnStillActive(resumedThreadId, activeTurnId))) {
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
                threadTurnRef.current = {
                  threadId: resumedThreadId,
                  turnId: activeTurnId ?? '',
                };
                setResumedThreadId(resumedThreadId);
                setStreamingThreadIds((prev) => new Set(prev).add(resumedThreadId));
                setPendingComposerStream(true);
                syncRecoveryContext();
                toast.warning(t('composer.turnStillRunning'));
                await pollThreadTurnEvents(
                  resumedThreadId,
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
      taskTypePreference,
      modelParams,
      desktopHost,
      streamControllersRef,
      threadTurnRef,
      streamSessionRef,
      setStreamingThreadIds,
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
    ],
  );

  return { handleSend, resetTurnPersistState };
}
