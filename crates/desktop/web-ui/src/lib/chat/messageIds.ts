/**
 * Collision-safe UI message ids.
 *
 * Historical bug: `useTurnSend` and session/cache restore both minted `msg-1`,
 * `msg-2`, … from independent counters that reset on reload. After thread
 * replay restore, a follow-up send reused those ids — `patchStreamingAssistant`
 * then updated EVERY row with that id, so the previous and current assistant
 * bubbles showed the same streaming text (dual「生成中」).
 */

let seq = 0;

/** Note ids already present (restore / cache) so counters advance past them. */
export function noteExistingMessageIds(messages: readonly { id?: string }[]): void {
  for (const m of messages) {
    const id = m.id?.trim();
    if (!id) continue;
    const match = /^msg-(\d+)$/.exec(id);
    if (match) {
      const n = Number(match[1]);
      if (Number.isFinite(n) && n > seq) {
        seq = n;
      }
    }
  }
}

/**
 * Allocate a new message id. Always unique vs previously noted `msg-N` ids and
 * vs other allocations in this JS realm (timestamp + seq + random).
 */
export function allocateMessageId(prefix = 'msg'): string {
  seq += 1;
  const rand =
    typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
      ? crypto.randomUUID().slice(0, 8)
      : Math.random().toString(36).slice(2, 10);
  return `${prefix}-${seq}-${Date.now().toString(36)}-${rand}`;
}

/** @internal test helper */
export function resetMessageIdStateForTests(): void {
  seq = 0;
}
