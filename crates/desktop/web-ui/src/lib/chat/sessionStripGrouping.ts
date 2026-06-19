/** One row in the session strip (sidebar list). */
export interface SessionStripSession {
  id: string;
  name: string;
  created_at?: number;
  updated_at?: number;
}

/** Default visible rows per date group before "More". */
export const SESSIONS_VISIBLE_PER_DAY = 5;

export type SessionStripDateGroup = {
  /** Display label, e.g. `2026/06/19`. */
  dateKey: string;
  /** Midnight local ms — newest groups first. */
  sortKey: number;
  sessions: SessionStripSession[];
};

function sessionActivityMs(session: SessionStripSession): number {
  return session.updated_at ?? session.created_at ?? 0;
}

export function formatSessionDateKey(ms: number): string {
  const d = new Date(ms);
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, '0');
  const day = String(d.getDate()).padStart(2, '0');
  return `${y}/${m}/${day}`;
}

function startOfLocalDayMs(ms: number): number {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

/** Group sessions by local calendar day (`updated_at`, then `created_at`). */
export function groupSessionsByDate(sessions: SessionStripSession[]): SessionStripDateGroup[] {
  const buckets = new Map<string, { sortKey: number; sessions: SessionStripSession[] }>();

  for (const session of sessions) {
    const activity = sessionActivityMs(session);
    const ts = activity > 0 ? activity : Date.now();
    const dateKey = formatSessionDateKey(ts);
    let bucket = buckets.get(dateKey);
    if (!bucket) {
      bucket = { sortKey: startOfLocalDayMs(ts), sessions: [] };
      buckets.set(dateKey, bucket);
    }
    bucket.sessions.push(session);
  }

  const groups: SessionStripDateGroup[] = [];
  for (const [dateKey, bucket] of buckets) {
    bucket.sessions.sort((a, b) => sessionActivityMs(b) - sessionActivityMs(a));
    groups.push({ dateKey, sortKey: bucket.sortKey, sessions: bucket.sessions });
  }
  groups.sort((a, b) => b.sortKey - a.sortKey);
  return groups;
}
