/** Pure helpers for chat transcript stick-to-bottom + collapse anti-shake. */

export const CHAT_STICK_BOTTOM_THRESHOLD_PX = 120;

export function isStickToBottom(
  scrollHeight: number,
  scrollTop: number,
  clientHeight: number,
  thresholdPx: number = CHAT_STICK_BOTTOM_THRESHOLD_PX,
): boolean {
  return scrollHeight - scrollTop - clientHeight <= thresholdPx;
}

/**
 * After transcript content resizes (stream growth or activity collapse), compute
 * the next scrollTop so the view either stays glued to the bottom or keeps the
 * same visual anchor when height shrinks above the viewport.
 */
export function nextScrollTopAfterContentResize(opts: {
  prevHeight: number;
  newHeight: number;
  prevScrollTop: number;
  clientHeight: number;
  stickBottom: boolean;
}): number {
  const { prevHeight, newHeight, prevScrollTop, clientHeight, stickBottom } = opts;
  if (newHeight === prevHeight) return prevScrollTop;
  if (stickBottom && newHeight > prevHeight) {
    return newHeight - clientHeight;
  }
  // On shrink, never chase the new bottom — keeps mid-list anchor stable.
  return prevScrollTop;
}

/** Restore an element's screen Y after local accordion collapse. */
export function scrollTopToPinElementTop(
  scroller: { scrollTop: number },
  pinTopScreenY: number,
  topAfterScreenY: number,
): number {
  return scroller.scrollTop + (topAfterScreenY - pinTopScreenY);
}
