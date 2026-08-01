/**
 * Apply timeline SSE events into a background thread's StreamContext so
 * switch-back shows live thinking/tools without waiting for full rebuild.
 */
import type { NormalizedStreamEvent } from '../../api/streamNormalize';
import { lastAssistantMessageId } from '../../lib/chat/activeTurnStreamUi';
import { finalizeInactiveAssistants } from '../../lib/chat/finalizeInactiveAssistants';
import { createEmptyTimelineState } from '../../lib/chat/timeline/turnTimelineReducer';
import type { StreamContextRegistry } from '../useStreamContextRegistry';
import type { TurnChatMessage } from '../useTurnSend';
import {
  applyStreamEventToMessages,
  isTimelineStreamEvent,
} from './applyStreamEventToMessages';

let bgAssistantSeq = 0;

function nextBackgroundAssistantId(): string {
  bgAssistantSeq += 1;
  return `asst-bg-${Date.now()}-${bgAssistantSeq}`;
}

function ensureStreamingAssistant(messages: TurnChatMessage[]): {
  messages: TurnChatMessage[];
  targetId: string;
} {
  const lastId = lastAssistantMessageId(messages);
  if (lastId) {
    const last = messages.find((m) => m.id === lastId);
    if (last?.role === 'assistant' && last.isStreaming) {
      return { messages, targetId: lastId };
    }
  }
  // Never rebind a completed assistant into the live stream — that overwrites
  // the previous turn's content with the new turn's deltas (dual identical
  // bubbles after thread-replay restore + continue). Always append a fresh row.
  const targetId = nextBackgroundAssistantId();
  return {
    messages: [
      ...finalizeInactiveAssistants(messages, null),
      {
        id: targetId,
        role: 'assistant',
        content: '',
        blocks: [],
        isStreaming: true,
      },
    ],
    targetId,
  };
}

/** Whether this norm should update a background thread transcript. */
export function isBackgroundTimelineContentEvent(
  kind: NormalizedStreamEvent['kind'],
): boolean {
  return (
    kind === 'thinking_delta' ||
    kind === 'message_delta' ||
    kind === 'message_segment' ||
    kind === 'tool_started' ||
    kind === 'tool_progress' ||
    kind === 'tool_completed'
  );
}

/**
 * Patch `threadId`'s registry messages + timelineState for a content/timeline event.
 * No-op for non-timeline kinds.
 */
export function applyBackgroundTimelineEvent(
  registry: StreamContextRegistry,
  threadId: string,
  sessionId: string | null | undefined,
  norm: NormalizedStreamEvent,
  options?: { finalize?: boolean; currentToolId?: string | null },
): void {
  const tid = threadId.trim();
  if (!tid || !isTimelineStreamEvent(norm.kind)) {
    return;
  }
  if (norm.kind === 'turn_started') {
    registry.ensureContext(tid, sessionId ?? null);
    registry.patchContext(tid, {
      timelineState: createEmptyTimelineState(),
      isStreaming: true,
      sessionId: sessionId ?? registry.getContext(tid)?.sessionId ?? null,
    });
    return;
  }
  if (
    !isBackgroundTimelineContentEvent(norm.kind) &&
    norm.kind !== 'turn_completed' &&
    norm.kind !== 'error'
  ) {
    return;
  }

  registry.ensureContext(tid, sessionId ?? null);
  const ctx = registry.getContext(tid)!;
  const ensured = ensureStreamingAssistant(ctx.messages as TurnChatMessage[]);
  const timelineState = ctx.timelineState ?? createEmptyTimelineState();
  const result = applyStreamEventToMessages(ensured.messages, timelineState, norm, {
    streamTargetId: ensured.targetId,
    currentToolId: options?.currentToolId,
    finalize: options?.finalize,
  });
  const finalize =
    options?.finalize === true ||
    norm.kind === 'turn_completed' ||
    norm.kind === 'error';

  registry.patchContext(tid, {
    messages: result.messages,
    timelineState: result.timelineState,
    isStreaming: !finalize,
    sessionId: sessionId ?? ctx.sessionId,
  });
}
