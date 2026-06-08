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
const STORAGE_KEY = 'zagens-desktop-session-ui-v1';
const STORAGE_META_KEY = 'zagens-desktop-session-ui-meta-v1';

type CacheAccessMeta = {
  accessedAt: Record<string, number>;
};

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

function readAccessMeta(): CacheAccessMeta {
  try {
    const raw = localStorage.getItem(STORAGE_META_KEY);
    if (!raw) {
      return { accessedAt: {} };
    }
    const parsed = JSON.parse(raw) as CacheAccessMeta;
    if (!parsed || typeof parsed !== 'object' || !parsed.accessedAt) {
      return { accessedAt: {} };
    }
    return parsed;
  } catch {
    return { accessedAt: {} };
  }
}

function writeAccessMeta(meta: CacheAccessMeta): void {
  try {
    localStorage.setItem(STORAGE_META_KEY, JSON.stringify(meta));
  } catch {
    /* quota / private mode */
  }
}

function touchSessionAccess(sessionId: string): void {
  const meta = readAccessMeta();
  meta.accessedAt[sessionId] = Date.now();
  writeAccessMeta(meta);
}

function evictOldestSessions(
  store: Record<string, CachedUiMessage[]>,
  maxSessions: number,
): void {
  const meta = readAccessMeta();
  const ids = Object.keys(store);
  while (ids.length > maxSessions) {
    let oldestId = ids[0];
    let oldestAt = meta.accessedAt[oldestId] ?? 0;
    for (const id of ids) {
      const at = meta.accessedAt[id] ?? 0;
      if (at < oldestAt) {
        oldestAt = at;
        oldestId = id;
      }
    }
    delete store[oldestId];
    delete meta.accessedAt[oldestId];
    ids.splice(ids.indexOf(oldestId), 1);
  }
  writeAccessMeta(meta);
}

function persistDisk(sessionId: string, messages: CachedUiMessage[]): void {
  if (!sessionId || messages.length === 0) {
    return;
  }
  const store = readDiskStore();
  store[sessionId] = cloneMessages(messages);
  touchSessionAccess(sessionId);
  evictOldestSessions(store, MAX_CACHED_SESSIONS);
  writeDiskStore(store);
}

/** Load UI snapshot from localStorage (survives app restart). */
export function loadPersistedSessionUiMessages(sessionId: string): CachedUiMessage[] | undefined {
  const hit = readDiskStore()[sessionId];
  if (!hit?.length) {
    return undefined;
  }
  touchSessionAccess(sessionId);
  return cloneMessages(hit);
}

/** Store a snapshot; evict least-recently-used entries when over capacity. */
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
  touchSessionAccess(sessionId);
  while (cache.size > MAX_CACHED_SESSIONS) {
    const meta = readAccessMeta();
    let oldestId: string | undefined;
    let oldestAt = Number.POSITIVE_INFINITY;
    for (const id of cache.keys()) {
      const at = meta.accessedAt[id] ?? 0;
      if (at < oldestAt) {
        oldestAt = at;
        oldestId = id;
      }
    }
    if (!oldestId) {
      const first = cache.keys().next().value;
      if (!first) {
        break;
      }
      cache.delete(first);
      continue;
    }
    cache.delete(oldestId);
  }
}

export function getCachedSessionUiMessages(
  cache: Map<string, CachedUiMessage[]>,
  sessionId: string,
): CachedUiMessage[] | undefined {
  const mem = cache.get(sessionId);
  if (mem?.length) {
    touchSessionAccess(sessionId);
    return cloneMessages(mem);
  }
  return loadPersistedSessionUiMessages(sessionId);
}
