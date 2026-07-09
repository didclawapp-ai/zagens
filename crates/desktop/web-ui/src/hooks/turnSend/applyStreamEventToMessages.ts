import type { NormalizedStreamEvent } from '../../api/streamNormalize';
import type { TurnChatMessage } from '../useTurnSend';
import { blocksToLegacyFields } from '../../lib/chat/timeline/legacyMessageAdapter';
import {
  applyTimelineEvent,
  createEmptyTimelineState,
  finalizeTimelineBlocks,
} from '../../lib/chat/timeline/turnTimelineReducer';
import type { TimelineState } from '../../lib/chat/timeline/turnBlockTypes';

export type { TimelineState } from '../../lib/chat/timeline/turnBlockTypes';

export type ApplyStreamEventOptions = {
  streamTargetId: string;
  currentToolId?: string | null;
  finalize?: boolean;
};

const TIMELINE_STREAM_EVENTS = new Set<NormalizedStreamEvent['kind']>([
  'turn_started',
  'thinking_delta',
  'message_delta',
  'message_segment',
  'tool_started',
  'tool_progress',
  'tool_completed',
  'turn_completed',
  'error',
]);

export function isTimelineStreamEvent(kind: NormalizedStreamEvent['kind']): boolean {
  return TIMELINE_STREAM_EVENTS.has(kind);
}

function patchStreamingAssistant(
  messages: TurnChatMessage[],
  targetId: string,
  timelineState: TimelineState,
  streaming: boolean,
): TurnChatMessage[] {
  const blocks = streaming
    ? timelineState.blocks
    : finalizeTimelineBlocks(timelineState.blocks);
  const legacy = blocksToLegacyFields(blocks);
  return messages.map((m) => {
    if (m.id !== targetId) return m;
    return {
      ...m,
      blocks,
      content: legacy.content,
      thinking: legacy.thinking,
      tools: legacy.tools,
      isStreaming: streaming,
    };
  });
}

export function applyStreamEventToMessages(
  messages: TurnChatMessage[],
  timelineState: TimelineState,
  norm: NormalizedStreamEvent,
  options: ApplyStreamEventOptions,
): { messages: TurnChatMessage[]; timelineState: TimelineState } {
  if (norm.kind === 'turn_started') {
    return {
      messages,
      timelineState: createEmptyTimelineState(),
    };
  }

  const nextState = applyTimelineEvent(timelineState, norm, {
    currentToolId: options.currentToolId,
  });

  const finalize =
    options.finalize === true ||
    norm.kind === 'turn_completed' ||
    norm.kind === 'error';

  const nextMessages = patchStreamingAssistant(
    messages,
    options.streamTargetId,
    nextState,
    !finalize && messages.find((m) => m.id === options.streamTargetId)?.isStreaming !== false,
  );

  return { messages: nextMessages, timelineState: nextState };
}
