/**
 * Apply a restored transcript without clobbering a turn the user already started
 * while thread replay was still in flight (app restart → continue conversation).
 */
import { anyAssistantStreaming } from './activeTurnStreamUi';
import { finalizeInactiveAssistants } from './finalizeInactiveAssistants';
import { noteExistingMessageIds } from './messageIds';
import { mergeThreadTranscript } from '../../hooks/turnSend/completeStreamUi';
import type { TurnChatMessage } from '../../hooks/useTurnSend';

function userMessageCount(messages: readonly { role: string }[]): number {
  return messages.reduce((n, m) => (m.role === 'user' ? n + 1 : n), 0);
}

export type ApplyRestoredChatMessagesOptions = {
  /**
   * True when the backend turn is still active and the last assistant must stay
   * in the live「生成中」layout (streaming reattach).
   */
  keepStreaming?: boolean;
};

/**
 * Merge `restored` into the live transcript.
 * - If the user already sent a newer prompt (more user rows) or an assistant is
 *   streaming, keep the live turn and only enrich from restored via transcript merge.
 * - Otherwise adopt restored; when idle, settle every assistant so replayed blocks
 *   cannot show a second「生成中」frame.
 */
export function applyRestoredChatMessages(
  prev: TurnChatMessage[],
  restored: TurnChatMessage[],
  options?: ApplyRestoredChatMessagesOptions,
): TurnChatMessage[] {
  if (restored.length === 0) {
    return prev;
  }

  noteExistingMessageIds(prev);
  noteExistingMessageIds(restored);

  const liveAhead =
    anyAssistantStreaming(prev) || userMessageCount(prev) > userMessageCount(restored);

  if (liveAhead) {
    return mergeThreadTranscript(prev, restored);
  }

  if (options?.keepStreaming) {
    return restored;
  }

  return finalizeInactiveAssistants(restored, null);
}
