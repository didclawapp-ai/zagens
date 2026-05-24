/**
 * Roving tabindex helpers for WAI-ARIA tablists (F3 a11y).
 */

import type { KeyboardEvent } from 'react';

export function nextTabListIndex(
  key: string,
  currentIndex: number,
  count: number,
): number | null {
  if (count <= 0) {
    return null;
  }
  switch (key) {
    case 'ArrowRight':
    case 'ArrowDown':
      return (currentIndex + 1) % count;
    case 'ArrowLeft':
    case 'ArrowUp':
      return (currentIndex - 1 + count) % count;
    case 'Home':
      return 0;
    case 'End':
      return count - 1;
    default:
      return null;
  }
}

/** Move focus + selection on Arrow/Home/End in a horizontal tablist. */
export function handleTabListKeyDown<T extends string>(
  e: KeyboardEvent,
  tabs: readonly T[],
  current: T,
  onSelect: (tab: T) => void,
  tabIdFor: (tab: T) => string,
): void {
  const idx = tabs.indexOf(current);
  if (idx < 0) {
    return;
  }
  const next = nextTabListIndex(e.key, idx, tabs.length);
  if (next == null) {
    return;
  }
  e.preventDefault();
  const tab = tabs[next]!;
  onSelect(tab);
  queueMicrotask(() => {
    document.getElementById(tabIdFor(tab))?.focus();
  });
}
