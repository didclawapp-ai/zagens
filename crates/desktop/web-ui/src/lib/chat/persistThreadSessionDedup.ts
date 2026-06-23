import { persistThreadSession } from '../../api/client';

type PersistResult = { session_id: string; message_count: number };

const inflightByThread = new Map<string, Promise<PersistResult>>();

/**
 * Serialize `persist-session` calls per thread so concurrent checkpoints
 * (turn_started, 60s tick, turn-complete) cannot each create a new sidebar row.
 */
export async function persistThreadSessionDeduped(
  threadId: string,
  sessionId?: string | null,
): Promise<PersistResult> {
  const tid = threadId.trim();
  if (!tid) {
    throw new Error('persistThreadSessionDeduped: threadId required');
  }

  const prev = inflightByThread.get(tid) ?? Promise.resolve(null as PersistResult | null);
  const run = prev
    .catch(() => null)
    .then((prior) => {
      let sid = sessionId?.trim() || undefined;
      if (!sid && prior?.session_id) {
        sid = prior.session_id;
      }
      return persistThreadSession(tid, sid ?? null);
    });

  inflightByThread.set(tid, run);
  try {
    return await run;
  } finally {
    if (inflightByThread.get(tid) === run) {
      inflightByThread.delete(tid);
    }
  }
}
