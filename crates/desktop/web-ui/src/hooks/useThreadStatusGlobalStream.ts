import { useEffect, useRef } from 'react';
import {
  subscribeGlobalThreadStatusEvents,
  type RuntimeConnectionState,
} from '../api/client';
import { normalizeDesktopStreamEvent } from '../api/streamNormalize';
import {
  applyThreadStatusEvent,
  applyThreadStatusSnapshot,
  normalizeThreadStreamStatus,
  type ThreadStreamStatus,
} from '../lib/chat/threadStatusStore';

const RECONNECT_DELAY_MS = 3000;

const SNAPSHOT_EVENT = 'thread.status.snapshot';

type SnapshotRow = {
  thread_id?: unknown;
  status?: unknown;
  seq?: unknown;
  turn_id?: unknown;
};

type SnapshotItem = {
  threadId: string;
  status: ThreadStreamStatus;
  seq: number;
  turnId?: string;
};

/** Parse a `thread.status.snapshot` frame into reconcile items (best-effort). */
function parseSnapshotFrame(data: string): SnapshotItem[] | null {
  let parsed: { threads?: unknown };
  try {
    parsed = JSON.parse(data) as { threads?: unknown };
  } catch {
    return null;
  }
  const rows = Array.isArray(parsed.threads) ? (parsed.threads as SnapshotRow[]) : [];
  const items: SnapshotItem[] = [];
  for (const row of rows) {
    const threadId = typeof row.thread_id === 'string' ? row.thread_id.trim() : '';
    const status = normalizeThreadStreamStatus(row.status);
    if (!threadId || !status) continue;
    items.push({
      threadId,
      status,
      seq: typeof row.seq === 'number' ? row.seq : 0,
      ...(typeof row.turn_id === 'string' ? { turnId: row.turn_id } : {}),
    });
  }
  return items;
}

/**
 * Always-on global `thread.status` SSE (P1/P3). Feeds authoritative `threadStatusStore`.
 */
export function useThreadStatusGlobalStream(runtimeConn: RuntimeConnectionState): void {
  const abortRef = useRef<AbortController | null>(null);

  useEffect(() => {
    if (runtimeConn !== 'connected') {
      abortRef.current?.abort();
      abortRef.current = null;
      return;
    }

    const ac = new AbortController();
    abortRef.current = ac;

    const run = async () => {
      while (!ac.signal.aborted) {
        try {
          await subscribeGlobalThreadStatusEvents((ev) => {
            if (ac.signal.aborted) {
              return;
            }
            if (ev.event === SNAPSHOT_EVENT) {
              const items = parseSnapshotFrame(ev.data);
              if (items) {
                applyThreadStatusSnapshot(items);
              }
              return;
            }
            const norm = normalizeDesktopStreamEvent(ev);
            if (norm?.kind === 'thread_status') {
              applyThreadStatusEvent({
                threadId: norm.threadId,
                status: norm.status,
                seq: norm.seq,
                turnId: norm.turnId,
                source: 'global_status_sse',
              });
            }
          }, { signal: ac.signal });
        } catch {
          if (ac.signal.aborted) {
            return;
          }
          await new Promise((resolve) => setTimeout(resolve, RECONNECT_DELAY_MS));
        }
      }
    };

    void run();
    return () => {
      ac.abort();
    };
  }, [runtimeConn]);
}
