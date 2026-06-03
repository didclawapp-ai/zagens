/** Minimal message shape for active-turn UI rebind (avoids hook import cycles). */
export type StreamUiMessage = {
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

export function lastAssistantMessageId(messages: StreamUiMessage[]): string | undefined {
  for (let i = messages.length - 1; i >= 0; i--) {
    if (messages[i].role === 'assistant') {
      return messages[i].id;
    }
  }
  return undefined;
}

function appendBannerLine(content: string, banner: string): string {
  const trimmed = content.trim();
  if (!trimmed) return banner;
  if (trimmed.includes(banner)) return content;
  return `[${banner}] ${content}`;
}

/** Mark one assistant as streaming; clear streaming flag on any other assistant rows. */
export function rebindStreamingAssistant(
  messages: StreamUiMessage[],
  targetId: string,
  banner?: string,
): StreamUiMessage[] {
  return messages.map((m) => {
    if (m.role === 'assistant' && m.id !== targetId && m.isStreaming) {
      return { ...m, isStreaming: false };
    }
    if (m.id !== targetId) return m;
    return {
      ...m,
      isStreaming: true,
      ...(banner ? { content: appendBannerLine(m.content, banner) } : {}),
    };
  });
}

export function markLastAssistantStreaming(messages: StreamUiMessage[]): {
  messages: StreamUiMessage[];
  assistantId: string | undefined;
} {
  const lastId = lastAssistantMessageId(messages);
  if (!lastId) {
    return { messages, assistantId: undefined };
  }
  return {
    messages: rebindStreamingAssistant(messages, lastId),
    assistantId: lastId,
  };
}
