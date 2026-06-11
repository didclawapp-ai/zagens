/** Pick the richest chat snapshot when multiple restore sources disagree. */

import type { CachedUiMessage } from './sessionUiCache';

export type SessionMessageCandidate = {
  source: 'cache' | 'session' | 'thread';
  messages: CachedUiMessage[];
};

/** Weight message count lightly — fragmented session API must not beat one rich assistant bubble. */
const MESSAGE_COUNT_WEIGHT = 40;
/** Large bonus when an assistant bubble carries restorable meta UI (tools / thinking). */
const META_BLOCK_BONUS = 12_000;
const TOOL_ENTRY_BONUS = 800;

const SOURCE_PRIORITY: Record<SessionMessageCandidate['source'], number> = {
  thread: 3,
  cache: 2,
  session: 1,
};

function assistantHasMeta(m: CachedUiMessage): boolean {
  return Boolean(m.thinking?.trim()) || Boolean(m.tools?.length);
}

/** Score a snapshot — higher means more complete for UI restore. */
export function sessionMessageRichness(messages: CachedUiMessage[]): number {
  if (messages.length === 0) {
    return 0;
  }
  let score = messages.length * MESSAGE_COUNT_WEIGHT;
  for (const m of messages) {
    score += m.content.length;
    const thinkingLen = m.thinking?.length ?? 0;
    score += thinkingLen;
    if (thinkingLen > 0) {
      score += META_BLOCK_BONUS;
    }
    if (m.tools?.length) {
      score += META_BLOCK_BONUS;
      score += m.tools.length * TOOL_ENTRY_BONUS;
      for (const t of m.tools) {
        score += (t.output?.length ?? 0) + t.input.length;
      }
    }
  }
  return score;
}

/**
 * Prefer the candidate with the highest richness score.
 * On a tie, prefer authoritative sources: thread replay, then local cache, then session JSON.
 */
export function pickBestSessionMessages(
  candidates: SessionMessageCandidate[],
): CachedUiMessage[] {
  return pickBestSessionMessagesWithSource(candidates).messages;
}

export function pickBestSessionMessagesWithSource(
  candidates: SessionMessageCandidate[],
): { messages: CachedUiMessage[]; source: SessionMessageCandidate['source'] | null } {
  let best: CachedUiMessage[] = [];
  let bestSource: SessionMessageCandidate['source'] | null = null;
  let bestScore = -1;
  let bestPriority = -1;
  for (const c of candidates) {
    if (c.messages.length === 0) {
      continue;
    }
    const score = sessionMessageRichness(c.messages);
    const priority = SOURCE_PRIORITY[c.source];
    if (score > bestScore || (score === bestScore && priority > bestPriority)) {
      bestScore = score;
      bestPriority = priority;
      best = c.messages;
      bestSource = c.source;
    }
  }
  return { messages: best, source: bestSource };
}

/** True when any assistant message still has tools or thinking blocks for the meta UI. */
export function snapshotHasAssistantMeta(messages: CachedUiMessage[]): boolean {
  return messages.some((m) => m.role === 'assistant' && assistantHasMeta(m));
}
