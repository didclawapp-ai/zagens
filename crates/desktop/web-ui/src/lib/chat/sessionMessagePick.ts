/** Pick the richest chat snapshot when multiple restore sources disagree. */

import type { CachedUiMessage } from './sessionUiCache';

export type SessionMessageCandidate = {
  source: 'cache' | 'session' | 'thread';
  messages: CachedUiMessage[];
};

/** Score a snapshot — higher means more complete for UI restore. */
export function sessionMessageRichness(messages: CachedUiMessage[]): number {
  if (messages.length === 0) {
    return 0;
  }
  let score = messages.length * 10_000;
  for (const m of messages) {
    score += m.content.length;
    score += m.thinking?.length ?? 0;
    if (m.tools?.length) {
      score += m.tools.length * 500;
      for (const t of m.tools) {
        score += (t.output?.length ?? 0) + t.input.length;
      }
    }
  }
  return score;
}

/**
 * Prefer the candidate with the highest richness score.
 * On a tie, later candidates win so authoritative server replay can override stale cache.
 */
export function pickBestSessionMessages(
  candidates: SessionMessageCandidate[],
): CachedUiMessage[] {
  let best: CachedUiMessage[] = [];
  let bestScore = -1;
  for (const c of candidates) {
    if (c.messages.length === 0) {
      continue;
    }
    const score = sessionMessageRichness(c.messages);
    if (score >= bestScore) {
      bestScore = score;
      best = c.messages;
    }
  }
  return best;
}
