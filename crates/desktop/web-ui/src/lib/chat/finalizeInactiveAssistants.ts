import {
  lastAssistantMessageId,
  rebindStreamingAssistant,
  clearStreamingAssistants,
  type StreamUiMessage,
} from './activeTurnStreamUi';
import {
  blocksToLegacyFields,
  legacyFieldsToBlocks,
} from './timeline/legacyMessageAdapter';
import { finalizeTimelineBlocks } from './timeline/turnTimelineReducer';
import type { TurnBlock } from './timeline/turnBlockTypes';

/** Message shape needed to settle stale in-flight timeline blocks. */
export type FinalizableChatMessage = StreamUiMessage & {
  blocks?: TurnBlock[];
  thinkingIncomplete?: boolean;
};

function assistantNeedsFinalize(message: FinalizableChatMessage): boolean {
  if (message.role !== 'assistant') return false;
  if (message.isStreaming) return true;
  const blocks =
    message.blocks && message.blocks.length > 0
      ? message.blocks
      : legacyFieldsToBlocks(message, message.id);
  return blocks.some(
    (b) =>
      (b.kind === 'thinking' && b.streaming !== false) ||
      (b.kind === 'text' && b.streaming !== false) ||
      (b.kind === 'tool' && b.status === 'running'),
  );
}

/** Settle thinking/text/tool in-flight state on one assistant bubble. */
export function finalizeAssistantMessage<T extends FinalizableChatMessage>(message: T): T {
  if (message.role !== 'assistant') return message;
  const sourceBlocks =
    message.blocks && message.blocks.length > 0
      ? message.blocks
      : legacyFieldsToBlocks(message, message.id);
  const nextBlocks = finalizeTimelineBlocks(sourceBlocks);
  const legacy = blocksToLegacyFields(nextBlocks);
  return {
    ...message,
    isStreaming: false,
    blocks: nextBlocks,
    content: legacy.content || message.content,
    thinking: legacy.thinking ?? message.thinking,
    tools: legacy.tools ?? message.tools,
  };
}

/**
 * Ensure at most one assistant may keep in-flight / streaming UI.
 * When `activeAssistantId` is null/undefined, finalize every assistant
 * (e.g. before starting a new user turn).
 */
export function finalizeInactiveAssistants<T extends FinalizableChatMessage>(
  messages: T[],
  activeAssistantId?: string | null,
): T[] {
  const keepId = activeAssistantId === undefined ? null : activeAssistantId;
  let changed = false;
  const next = messages.map((m) => {
    if (m.role !== 'assistant') return m;
    if (keepId != null && m.id === keepId) {
      if (m.isStreaming) return m;
      // Active target may still be mid-turn without the flag yet — keep blocks.
      return m;
    }
    if (!assistantNeedsFinalize(m)) return m;
    changed = true;
    return finalizeAssistantMessage(m);
  });
  return changed ? next : messages;
}

/** Finalize every assistant except `targetId`, then mark that row streaming. */
export function rebindStreamingAssistantUi<T extends FinalizableChatMessage>(
  messages: T[],
  targetId: string,
  banner?: string,
): T[] {
  const settled = finalizeInactiveAssistants(messages, targetId);
  return rebindStreamingAssistant(settled, targetId, banner) as T[];
}

/** Clear streaming flags and settle any leftover in-flight blocks. */
export function clearStreamingAssistantsUi<T extends FinalizableChatMessage>(messages: T[]): T[] {
  const cleared = clearStreamingAssistants(messages) as T[];
  return finalizeInactiveAssistants(cleared, null);
}

/** Mark the last assistant streaming after settling older assistant rows. */
export function markLastAssistantStreamingUi<T extends FinalizableChatMessage>(
  messages: T[],
): { messages: T[]; assistantId: string | undefined } {
  const lastId = lastAssistantMessageId(messages);
  if (!lastId) {
    return { messages, assistantId: undefined };
  }
  return {
    messages: rebindStreamingAssistantUi(messages, lastId),
    assistantId: lastId,
  };
}
