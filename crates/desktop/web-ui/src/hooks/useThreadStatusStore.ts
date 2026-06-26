/**
 * P3 — React bindings for authoritative `threadStatusStore`.
 */
import { useMemo, useSyncExternalStore } from 'react';
import {
  getActiveThreadIdsFromStore,
  getThreadStatusEntry,
  getThreadStatusStoreRevision,
  isThreadStreamActive,
  subscribeThreadStatusStore,
} from '../lib/chat/threadStatusStore';

export function useThreadStatusRevision(): number {
  return useSyncExternalStore(
    subscribeThreadStatusStore,
    getThreadStatusStoreRevision,
    getThreadStatusStoreRevision,
  );
}

/** Active thread ids (streaming | awaiting_approval) from the authoritative store. */
export function useActiveThreadIds(): Set<string> {
  const revision = useThreadStatusRevision();
  return useMemo(() => getActiveThreadIdsFromStore(), [revision]);
}

export function useIsThreadStreaming(threadId: string | null | undefined): boolean {
  const revision = useThreadStatusRevision();
  const tid = threadId?.trim() ?? '';
  return useMemo(() => {
    if (!tid) return false;
    const entry = getThreadStatusEntry(tid);
    return entry != null && isThreadStreamActive(entry.status);
  }, [revision, tid]);
}
