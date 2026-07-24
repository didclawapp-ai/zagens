import type { TimelinePresentationItem } from '../../../lib/chat/timeline/timelinePresentationTypes';

/** Index of the last collapsed activity row, or -1 when none. */
export function trailingActivityIndex(items: readonly TimelinePresentationItem[]): number {
  for (let i = items.length - 1; i >= 0; i--) {
    if (items[i].kind === 'collapsed_tools') return i;
  }
  return -1;
}
