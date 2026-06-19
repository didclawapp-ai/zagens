/** Progress scroll viewport layout — aligned with deliverables/zagens-ui-minimal-demo.html */

export type ProgressState = 'done' | 'current' | 'pending';

export interface ProgressScrollItem {
  id: string;
  progress: ProgressState;
}

export const PROGRESS_ROW_H_PX = 18;
export const PROGRESS_ROW_GAP_PX = 3;
export const PROGRESS_ROW_STEP_PX = PROGRESS_ROW_H_PX + PROGRESS_ROW_GAP_PX;

export interface ProgressScrollLayout {
  openCount: number;
  visibleRows: number;
  viewportHeightPx: number;
  offsetPx: number;
  overflow: boolean;
  scrollTop: boolean;
  scrollBottom: boolean;
  allDone: boolean;
  focusIndex: number;
}

export function computeProgressScrollLayout(
  items: readonly ProgressScrollItem[],
  maxRows: number,
): ProgressScrollLayout {
  const safeMaxRows = Math.max(1, maxRows);
  const open = items.filter((item) => item.progress !== 'done');

  if (open.length === 0) {
    return {
      openCount: 0,
      visibleRows: 1,
      viewportHeightPx: PROGRESS_ROW_H_PX,
      offsetPx: 0,
      overflow: false,
      scrollTop: false,
      scrollBottom: false,
      allDone: true,
      focusIndex: 0,
    };
  }

  let focusIndex = open.findIndex((item) => item.progress === 'current');
  if (focusIndex < 0) {
    focusIndex = 0;
  }

  const overflow = open.length > safeMaxRows;
  const maxOffset = Math.max(0, (open.length - safeMaxRows) * PROGRESS_ROW_STEP_PX);
  const offsetPx = overflow ? Math.min(focusIndex * PROGRESS_ROW_STEP_PX, maxOffset) : 0;
  const visibleRows = Math.min(open.length, safeMaxRows);

  return {
    openCount: open.length,
    visibleRows,
    viewportHeightPx: visibleRows * PROGRESS_ROW_STEP_PX - PROGRESS_ROW_GAP_PX,
    offsetPx,
    overflow,
    scrollTop: overflow && offsetPx > 0,
    scrollBottom: overflow && offsetPx < maxOffset,
    allDone: false,
    focusIndex,
  };
}

export function isProgressItemVisible(
  item: ProgressScrollItem,
  items: readonly ProgressScrollItem[],
): boolean {
  return item.progress !== 'done' && items.some((row) => row.id === item.id);
}
