import type { SessionDetail } from '../../api/client';
import { nextUiMessageId, type UiMessage, type UiToolCall } from './sessionMessages';

/** Fallback when thread event replay is unavailable — text, thinking, and tool blocks. */
export function mapSessionDetailToMessages(detail: SessionDetail): UiMessage[] {
  const out: UiMessage[] = [];
  for (const m of detail.messages) {
    const role = m.role === 'user' || m.role === 'assistant' ? m.role : 'assistant';
    const textParts: string[] = [];
    const thinkingParts: string[] = [];
    const tools: UiToolCall[] = [];
    const blocks = m.content ?? [];

    for (let i = 0; i < blocks.length; i++) {
      const b = blocks[i];
      if (b.type === 'text' && b.text) {
        textParts.push(b.text);
      } else if (b.type === 'thinking' && b.text) {
        thinkingParts.push(b.text);
      } else if (b.type === 'tool_use' && b.id && b.name) {
        const next = blocks[i + 1];
        const output =
          next?.type === 'tool_result' && next.tool_use_id === b.id
            ? (next.content ?? '')
            : undefined;
        const isError =
          next?.type === 'tool_result' && next.tool_use_id === b.id
            ? Boolean(next.is_error)
            : false;
        if (next?.type === 'tool_result' && next.tool_use_id === b.id) {
          i += 1;
        }
        tools.push({
          id: b.id,
          name: b.name,
          input:
            b.input != null
              ? typeof b.input === 'string'
                ? b.input
                : JSON.stringify(b.input)
              : '',
          output,
          status: isError ? 'error' : 'done',
        });
      }
    }

    const content = textParts.join('\n').trim();
    const thinking = thinkingParts.join('\n').trim();
    if (content || thinking || tools.length > 0) {
      out.push({
        id: nextUiMessageId(),
        role,
        content,
        ...(thinking ? { thinking } : {}),
        ...(tools.length > 0 ? { tools } : {}),
      });
    }
  }
  return out;
}
