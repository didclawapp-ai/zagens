/**
 * Authoritative thread streaming status store (P3).
 * Single write entry for `thread.status` events with global seq monotonic dedup.
 */

/** Coarse thread lifecycle status mirrored from the server `thread.status` event. */
export type ThreadStreamStatus = 'streaming' | 'awaiting_approval' | 'idle' | 'error';

export function normalizeThreadStreamStatus(raw: unknown): ThreadStreamStatus | null {
  const status = typeof raw === 'string' ? raw.trim() : '';
  switch (status) {
    case 'streaming':
    case 'awaiting_approval':
    case 'idle':
    case 'error':
      return status;
    default:
      return null;
  }
}

export type ThreadStatusEntry = {
  status: ThreadStreamStatus;
  lastSeq: number;
  turnId?: string;
  updatedAt: number;
  source?: string;
};

export type ThreadStatusApplyResult =
  | { applied: true; previous: ThreadStatusEntry | null }
  | { applied: false; reason: 'stale_seq' | 'invalid' };

type Listener = () => void;

let entries = new Map<string, ThreadStatusEntry>();
const listeners = new Set<Listener>();
let revision = 0;

const SHADOW_LOG_PREFIX = '[threadStatusStore]';

export function isThreadStreamActive(status: ThreadStreamStatus): boolean {
  return status === 'streaming' || status === 'awaiting_approval';
}

export function getThreadStatusStoreRevision(): number {
  return revision;
}

export function getThreadStatusEntry(threadId: string): ThreadStatusEntry | undefined {
  return entries.get(threadId.trim());
}

/** Thread ids the store considers actively producing (streaming / awaiting approval). */
export function getActiveThreadIdsFromStore(): Set<string> {
  const out = new Set<string>();
  for (const [tid, entry] of entries) {
    if (isThreadStreamActive(entry.status)) {
      out.add(tid);
    }
  }
  return out;
}

export function applyThreadStatusEvent(params: {
  threadId: string;
  status: ThreadStreamStatus;
  seq?: number;
  turnId?: string;
  source?: string;
}): ThreadStatusApplyResult {
  const tid = params.threadId.trim();
  if (!tid) {
    return { applied: false, reason: 'invalid' };
  }

  const prev = entries.get(tid) ?? null;
  if (params.seq != null && prev != null && params.seq <= prev.lastSeq) {
    return { applied: false, reason: 'stale_seq' };
  }

  if (params.status === 'idle' || params.status === 'error') {
    if (prev && params.seq != null && params.seq <= prev.lastSeq) {
      return { applied: false, reason: 'stale_seq' };
    }
    if (prev) {
      entries.delete(tid);
      revision += 1;
      notify();
    }
    return { applied: true, previous: prev };
  }

  const next: ThreadStatusEntry = {
    status: params.status,
    lastSeq: params.seq ?? prev?.lastSeq ?? 0,
    updatedAt: Date.now(),
    ...(params.turnId ? { turnId: params.turnId } : {}),
    ...(params.source ? { source: params.source } : {}),
  };
  entries.set(tid, next);
  revision += 1;
  notify();
  return { applied: true, previous: prev };
}

/** Optimistic idle on user Stop — retains a high-seq tombstone so in-flight `streaming` events are ignored. */
export function applyOptimisticThreadStop(threadId: string, turnId?: string): void {
  const tid = threadId.trim();
  if (!tid) return;
  const prev = entries.get(tid);
  const seq = (prev?.lastSeq ?? 0) + 10_000;
  entries.set(tid, {
    status: 'idle',
    lastSeq: seq,
    updatedAt: Date.now(),
    source: 'optimistic_stop',
    ...(turnId ? { turnId } : {}),
  });
  revision += 1;
  notify();
}

/**
 * Reconcile the store against an authoritative connect/lag snapshot.
 *
 * The snapshot is the full set of currently-active threads. Besides applying
 * each entry, any store thread that is active but absent from the snapshot went
 * idle while this client was disconnected — clear it so the spinner / composer
 * lock (both derived from this store) do not ghost after reconnect.
 */
export function applyThreadStatusSnapshot(
  items: ReadonlyArray<{
    threadId: string;
    status: ThreadStreamStatus;
    seq: number;
    turnId?: string;
  }>,
): void {
  const snapshotActive = new Set<string>();
  for (const item of items) {
    const tid = item.threadId.trim();
    if (!tid) continue;
    if (isThreadStreamActive(item.status)) {
      snapshotActive.add(tid);
    }
    applyThreadStatusEvent({
      threadId: tid,
      status: item.status,
      seq: item.seq,
      turnId: item.turnId,
      source: 'snapshot',
    });
  }

  const stale: string[] = [];
  for (const [tid, entry] of entries) {
    if (isThreadStreamActive(entry.status) && !snapshotActive.has(tid)) {
      stale.push(tid);
    }
  }
  if (stale.length === 0) return;
  for (const tid of stale) {
    entries.delete(tid);
  }
  revision += 1;
  notify();
}

export function subscribeThreadStatusStore(listener: Listener): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function getThreadStatusSnapshot(): ReadonlyMap<string, ThreadStatusEntry> {
  return entries;
}

export type ThreadStatusDrift = {
  threadId: string;
  legacyInSet: boolean;
  storeActive: boolean;
  storeStatus: ThreadStreamStatus | null;
  storeLastSeq: number | null;
};

export function detectThreadStatusDrift(
  legacyStreamingThreadIds: Set<string>,
): ThreadStatusDrift[] {
  const drifts: ThreadStatusDrift[] = [];
  const allIds = new Set<string>([...legacyStreamingThreadIds, ...entries.keys()]);
  for (const tid of allIds) {
    const entry = entries.get(tid);
    const storeActive = entry != null && isThreadStreamActive(entry.status);
    const legacyInSet = legacyStreamingThreadIds.has(tid);
    if (storeActive !== legacyInSet) {
      drifts.push({
        threadId: tid,
        legacyInSet,
        storeActive,
        storeStatus: entry?.status ?? null,
        storeLastSeq: entry?.lastSeq ?? null,
      });
    }
  }
  return drifts;
}

/** Log drift between shadow store and legacy `streamingThreadIds` (dev-only). */
export function logThreadStatusDrift(
  legacyStreamingThreadIds: Set<string>,
  trigger?: string,
): void {
  if (!import.meta.env.DEV) {
    return;
  }
  const drifts = detectThreadStatusDrift(legacyStreamingThreadIds);
  if (drifts.length === 0) {
    return;
  }
  for (const drift of drifts) {
    console.warn(SHADOW_LOG_PREFIX, {
      trigger: trigger ?? 'unknown',
      ...drift,
    });
  }
}

function notify(): void {
  for (const listener of listeners) {
    listener();
  }
}

/** Test-only reset. */
export function resetThreadStatusStoreForTests(): void {
  entries = new Map();
  revision = 0;
  listeners.clear();
}
