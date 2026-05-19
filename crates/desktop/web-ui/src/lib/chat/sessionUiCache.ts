/** In-memory per-session chat UI snapshot (tools + thinking) for fast session switching. */

export interface CachedUiMessage {
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
}

const MAX_CACHED_SESSIONS = 24;

function cloneMessages(msgs: CachedUiMessage[]): CachedUiMessage[] {
  return msgs.map((m) => ({
    ...m,
    isStreaming: false,
    tools: m.tools?.map((t) => ({ ...t })),
  }));
}

/** Store a snapshot; evict oldest entries when over capacity. */
export function cacheSessionUiMessages(
  cache: Map<string, CachedUiMessage[]>,
  sessionId: string,
  messages: CachedUiMessage[],
): void {
  if (!sessionId || messages.length === 0) {
    return;
  }
  cache.set(sessionId, cloneMessages(messages));
  while (cache.size > MAX_CACHED_SESSIONS) {
    const oldest = cache.keys().next().value;
    if (!oldest) {
      break;
    }
    cache.delete(oldest);
  }
}

export function getCachedSessionUiMessages(
  cache: Map<string, CachedUiMessage[]>,
  sessionId: string,
): CachedUiMessage[] | undefined {
  const hit = cache.get(sessionId);
  return hit?.length ? cloneMessages(hit) : undefined;
}
