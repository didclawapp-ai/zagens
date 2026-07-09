import { replayThreadEvents } from '../../api/client';
import { normalizeDesktopStreamEvent } from '../../api/streamNormalize';
import type { TurnChatMessage } from '../../hooks/useTurnSend';
import { mergeAgentMessageSegment } from './formatAssistantContent';
import {
  mergeStreamingToolOutput,
  stringifyToolInput,
  toolOutputString,
} from './toolOutput';
import { blocksToLegacyFields } from './timeline/legacyMessageAdapter';
import {
  applyTimelineEvent,
  createEmptyTimelineState,
  finalizeTimelineBlocks,
} from './timeline/turnTimelineReducer';
import type { TimelineState } from './timeline/turnBlockTypes';
import { resetBlockIdCounter, type TurnBlock } from './timeline/turnBlockTypes';
import {
  nextUiMessageId,
  resetUiMessageIdCounter,
  type UiMessage,
  type UiToolCall,
} from './sessionMessages';

interface HistoryState {
  messages: TurnChatMessage[];
  currentAssistantId: string | null;
  timeline: TimelineState;
  currentToolId: string | null;
  pendingParagraphBreak: boolean;
}

function flushAssistant(state: HistoryState): void {
  const blocks = finalizeTimelineBlocks(state.timeline.blocks);
  const hasBody = blocks.length > 0;
  if (!hasBody) {
    state.currentAssistantId = null;
    state.currentToolId = null;
    state.timeline = createEmptyTimelineState();
    return;
  }
  const id = state.currentAssistantId ?? nextUiMessageId('asst');
  const legacy = blocksToLegacyFields(blocks);
  state.messages.push({
    id,
    role: 'assistant',
    content: legacy.content,
    ...(legacy.thinking ? { thinking: legacy.thinking } : {}),
    ...(legacy.tools ? { tools: legacy.tools } : {}),
    blocks,
  });
  state.currentAssistantId = null;
  state.currentToolId = null;
  state.timeline = createEmptyTimelineState();
  state.pendingParagraphBreak = false;
}

function ensureAssistant(state: HistoryState): void {
  if (!state.currentAssistantId) {
    state.currentAssistantId = nextUiMessageId('asst');
  }
}

function applyNormalized(
  state: HistoryState,
  norm: ReturnType<typeof normalizeDesktopStreamEvent>,
): void {
  if (!norm) return;
  switch (norm.kind) {
    case 'turn_started':
    case 'turn_completed':
      flushAssistant(state);
      break;
    case 'thinking_delta':
    case 'message_delta':
    case 'message_segment':
    case 'tool_started':
    case 'tool_progress':
    case 'tool_completed':
      ensureAssistant(state);
      if (norm.kind === 'tool_started') {
        state.currentToolId = norm.id;
      }
      if (norm.kind === 'message_delta' && state.pendingParagraphBreak) {
        const last = state.timeline.blocks[state.timeline.blocks.length - 1];
        if (last?.kind === 'text' && last.content.trim()) {
          state.timeline = applyTimelineEvent(state.timeline, {
            kind: 'message_delta',
            content: last.content.endsWith('\n') ? '\n' : '\n\n',
          });
        }
        state.pendingParagraphBreak = false;
      }
      state.timeline = applyTimelineEvent(state.timeline, norm, {
        currentToolId: state.currentToolId,
      });
      if (norm.kind === 'message_segment') {
        state.pendingParagraphBreak = true;
      }
      if (norm.kind === 'tool_completed') {
        if (state.currentToolId === norm.id) {
          state.currentToolId = null;
        }
      }
      break;
    default:
      break;
  }
}

function upsertToolFromItem(
  state: HistoryState,
  id: string,
  patch: Partial<UiToolCall> & { name?: string; input?: string },
): void {
  ensureAssistant(state);
  state.timeline = applyTimelineEvent(state.timeline, {
    kind: 'tool_started',
    id,
    name: patch.name ?? 'tool',
    input: patch.input ?? '',
  });
  if (patch.output != null || patch.status) {
    state.timeline = applyTimelineEvent(
      state.timeline,
      {
        kind: 'tool_completed',
        id,
        success: patch.status !== 'error',
        output: patch.output ?? '',
      },
      { currentToolId: id },
    );
  }
}

function applyRawRecord(state: HistoryState, ev: { event: string; data: string }): void {
  let j: Record<string, unknown>;
  try {
    j = JSON.parse(ev.data) as Record<string, unknown>;
  } catch {
    return;
  }
  const recordEvent = (j.event as string | undefined) ?? ev.event;
  const inner = j.payload as Record<string, unknown> | undefined;
  if (!inner?.item || typeof inner.item !== 'object') {
    return;
  }
  const item = inner.item as Record<string, unknown>;
  if (recordEvent !== 'item.completed' && recordEvent !== 'item.failed') {
    return;
  }
  const kind = String(item.kind ?? '');
  const detail = typeof item.detail === 'string' ? item.detail : '';
  const summary = typeof item.summary === 'string' ? item.summary : '';

  if (kind === 'user_message') {
    flushAssistant(state);
    const text = (detail || summary).trim();
    if (text) {
      state.messages.push({
        id: nextUiMessageId('user'),
        role: 'user',
        content: text,
      });
    }
    return;
  }

  if (kind === 'thinking') {
    const text = (detail || summary).trim();
    if (text) {
      ensureAssistant(state);
      state.timeline = applyTimelineEvent(state.timeline, {
        kind: 'thinking_delta',
        content: text,
      });
      state.timeline = {
        ...state.timeline,
        blocks: state.timeline.blocks.map((block: TurnBlock) =>
          block.kind === 'thinking'
            ? { ...block, streaming: false, status: 'done' as const }
            : block,
        ),
      };
    }
    return;
  }

  if (kind === 'agent_message') {
    const text = (detail || summary).trim();
    if (text) {
      ensureAssistant(state);
      state.timeline = applyTimelineEvent(state.timeline, {
        kind: 'message_segment',
        content: text,
      });
    }
    state.pendingParagraphBreak = true;
    return;
  }

  if (
    kind === 'tool_call' ||
    kind === 'file_change' ||
    kind === 'command_execution'
  ) {
    const tool = inner.tool as Record<string, unknown> | undefined;
    const id = String(tool?.id ?? item.id ?? nextUiMessageId('tool'));
    const name = String(tool?.name ?? kind);
    const input = tool?.input != null ? stringifyToolInput(tool.input) : '';
    const outStr = (detail || summary).trim();
    upsertToolFromItem(state, id, {
      name,
      input,
      output: outStr || undefined,
      status: recordEvent === 'item.failed' ? 'error' : 'done',
    });
  }
}

function applyEvent(state: HistoryState, ev: { event: string; data: string }): void {
  const norm = normalizeDesktopStreamEvent(ev);
  if (norm) {
    applyNormalized(state, norm);
    return;
  }
  applyRawRecord(state, ev);
}

/** Synchronous rebuild with interleaved `blocks` from raw SSE/event records. */
export function rebuildMessagesFromEventRecords(
  events: Array<{ event: string; data: string }>,
): TurnChatMessage[] {
  resetUiMessageIdCounter();
  resetBlockIdCounter();
  const state: HistoryState = {
    messages: [],
    currentAssistantId: null,
    timeline: createEmptyTimelineState(),
    currentToolId: null,
    pendingParagraphBreak: false,
  };
  for (const ev of events) {
    applyEvent(state, ev);
  }
  flushAssistant(state);
  return state.messages;
}

/**
 * Rebuild chat UI messages (including interleaved timeline blocks) from persisted
 * runtime thread events.
 */
export async function rebuildMessagesFromThreadEvents(
  threadId: string,
  options?: { signal?: AbortSignal },
): Promise<UiMessage[]> {
  resetUiMessageIdCounter();
  resetBlockIdCounter();
  const state: HistoryState = {
    messages: [],
    currentAssistantId: null,
    timeline: createEmptyTimelineState(),
    currentToolId: null,
    pendingParagraphBreak: false,
  };

  await replayThreadEvents(
    threadId,
    0,
    (ev) => {
      applyEvent(state, ev);
    },
    { signal: options?.signal, waitForStreamClose: true },
  );

  flushAssistant(state);
  return state.messages;
}
