import type { SessionDetail } from '../../api/client';

export interface UiMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  thinking?: string;
  tools?: UiToolCall[];
}

export interface UiToolCall {
  id: string;
  name: string;
  input: string;
  output?: string;
  status: 'running' | 'done' | 'error';
}

let msgCounter = 0;
export function nextUiMessageId(prefix = 'msg'): string {
  msgCounter += 1;
  return `${prefix}-${msgCounter}`;
}

/** Reset counter when loading a fresh session (stable ids come from thread items when available). */
export function resetUiMessageIdCounter(): void {
  msgCounter = 0;
}

/** Fallback when thread event replay is unavailable — text/thinking only (legacy session API). */
export function mapSessionDetailToMessages(detail: SessionDetail): UiMessage[] {
  const out: UiMessage[] = [];
  for (const m of detail.messages) {
    const role = m.role === 'user' || m.role === 'assistant' ? m.role : 'assistant';
    const textParts: string[] = [];
    const thinkingParts: string[] = [];
    for (const b of m.content || []) {
      if (b.type === 'text' && b.text) {
        textParts.push(b.text);
      } else if (b.type === 'thinking' && b.text) {
        thinkingParts.push(b.text);
      }
    }
    const content = textParts.join('\n').trim();
    const thinking = thinkingParts.join('\n').trim();
    if (content || thinking) {
      out.push({
        id: nextUiMessageId(),
        role,
        content,
        ...(thinking ? { thinking } : {}),
      });
    }
  }
  return out;
}
