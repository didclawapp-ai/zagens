/** In-memory + localStorage per-session chat UI snapshot (tools + thinking). */

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
const STORAGE_KEY = 'deepseek-desktop-session-ui-v1';

function cloneMessages(msgs: CachedUiMessage[]): CachedUiMessage[] {
  return msgs.map((m) => ({
    ...m,
    isStreaming: false,
    tools: m.tools?.map((t) => ({ ...t })),
  }));
}

function readDiskStore(): Record<string, CachedUiMessage[]> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      return {};
    }
    const parsed = JSON.parse(raw) as Record<string, CachedUiMessage[]>;
    return parsed && typeof parsed === 'object' ? parsed : {};
  } catch {
    return {};
  }
}

function writeDiskStore(store: Record<string, CachedUiMessage[]>): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(store));
  } catch {
    /* quota / private mode */
  }
}

function persistDisk(sessionId: string, messages: CachedUiMessage[]): void {
  if (!sessionId || messages.length === 0) {
    return;
  }
  const store = readDiskStore();
  store[sessionId] = cloneMessages(messages);
  const keys = Object.keys(store);
  while (keys.length > MAX_CACHED_SESSIONS) {
    const oldest = keys.shift();
    if (!oldest) {
      break;
    }
    delete store[oldest];
  }
  writeDiskStore(store);
}

/** Load UI snapshot from localStorage (survives app restart). */
export function loadPersistedSessionUiMessages(sessionId: string): CachedUiMessage[] | undefined {
  const hit = readDiskStore()[sessionId];
  return hit?.length ? cloneMessages(hit) : undefined;
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
  const cloned = cloneMessages(messages);
  cache.set(sessionId, cloned);
  persistDisk(sessionId, cloned);
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
  const mem = cache.get(sessionId);
  if (mem?.length) {
    return cloneMessages(mem);
  }
  return loadPersistedSessionUiMessages(sessionId);
}
