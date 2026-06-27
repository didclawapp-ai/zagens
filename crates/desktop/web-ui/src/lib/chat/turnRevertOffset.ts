/**
 * Map a user message's `depth_from_tail` (fork API) to `turn_offset` for
 * `POST …/workspace/revert-turn` (pre-turn snapshot, newest = 1).
 */
export function turnOffsetFromDepthFromTail(depthFromTail: number): number {
  return depthFromTail + 1;
}
