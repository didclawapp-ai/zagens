import type { TurnBlock } from './timeline/turnBlockTypes';
import { allocateMessageId, resetMessageIdStateForTests } from './messageIds';

export interface UiMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  thinking?: string;
  tools?: UiToolCall[];
  blocks?: TurnBlock[];
  isStreaming?: boolean;
  /** Replay lacked persisted thinking (items-only / events missing). */
  thinkingIncomplete?: boolean;
}

export interface UiToolCall {
  id: string;
  name: string;
  input: string;
  output?: string;
  status: 'running' | 'done' | 'error';
}

export function nextUiMessageId(prefix = 'msg'): string {
  return allocateMessageId(prefix);
}

/** Reset counter when loading a fresh session (stable ids come from thread items when available). */
export function resetUiMessageIdCounter(): void {
  resetMessageIdStateForTests();
}

export { mapSessionDetailToMessages } from './mapSessionDetailToMessages';
