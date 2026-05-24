/** Messages used for backtrack depth calculation (user role only). */
export function userMessageIds(messages: Array<{ id: string; role: string }>): string[] {
  return messages.filter((m) => m.role === 'user').map((m) => m.id);
}

/** Whether `messageId` is the chronologically last user message in the transcript. */
export function isLastUserMessage(
  messages: Array<{ id: string; role: string }>,
  messageId: string,
): boolean {
  const ids = userMessageIds(messages);
  return ids.length > 0 && ids[ids.length - 1] === messageId;
}

/**
 * Map a user message to `depth_from_tail` for `POST .../fork-at-user-message`.
 * Returns `null` when the id is not a user message.
 */
export function depthFromTailForUserMessage(
  messages: Array<{ id: string; role: string }>,
  messageId: string,
): number | null {
  const userMsgs = messages.filter((m) => m.role === 'user');
  const idx = userMsgs.findIndex((m) => m.id === messageId);
  if (idx < 0) {
    return null;
  }
  return userMsgs.length - 1 - idx;
}
