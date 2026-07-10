import type {
  TimelinePresentationItem,
  TimelinePresentationRoot,
  TimelineStepGroup,
} from './timelinePresentationTypes';

/** Step contains assistant prose the user should still see after the turn settles. */
export function stepHasVisibleProse(items: readonly TimelinePresentationItem[]): boolean {
  return items.some(
    (item) =>
      item.kind === 'block' &&
      item.block.kind === 'text' &&
      item.block.content.trim().length > 0,
  );
}

export function isStepGroup(
  item: TimelinePresentationRoot,
): item is TimelineStepGroup {
  return typeof item === 'object' && item !== null && 'kind' in item && item.kind === 'step';
}

export type SettledFlatSegment =
  | { kind: 'process'; id: string; items: TimelinePresentationItem[] }
  | { kind: 'final'; id: string; item: TimelinePresentationItem };

export type SettledPresentationSegment =
  | {
      kind: 'process';
      id: string;
      items: TimelinePresentationItem[];
      stepCount: number;
    }
  | { kind: 'final-step'; id: string; step: TimelineStepGroup }
  | { kind: 'final-item'; id: string; item: TimelinePresentationItem };

function presentationItemId(item: TimelinePresentationItem): string {
  if (item.kind === 'collapsed_tools') return item.id;
  return item.block.id;
}

/**
 * After the turn ends, keep final text visible and bundle tool/thinking rows
 * into collapsible process segments (thr_ea9c settled view).
 */
export function partitionFlatPresentationForSettledView(
  items: readonly TimelinePresentationItem[],
): SettledFlatSegment[] {
  const out: SettledFlatSegment[] = [];
  let processBuf: TimelinePresentationItem[] = [];
  let processStartId: string | null = null;

  const flushProcess = () => {
    if (processBuf.length === 0) return;
    out.push({
      kind: 'process',
      id: `process-${processStartId ?? presentationItemId(processBuf[0])}`,
      items: processBuf,
    });
    processBuf = [];
    processStartId = null;
  };

  for (const item of items) {
    const isFinalText =
      item.kind === 'block' &&
      item.block.kind === 'text' &&
      item.block.content.trim().length > 0;

    if (isFinalText) {
      flushProcess();
      out.push({ kind: 'final', id: item.block.id, item });
      continue;
    }

    if (processBuf.length === 0) {
      processStartId = presentationItemId(item);
    }
    processBuf.push(item);
  }
  flushProcess();
  return out;
}

/**
 * Settled step view: collapse consecutive tool-only steps into one process bundle;
 * keep steps that carry final prose expanded.
 */
export function partitionPresentationForSettledView(
  roots: readonly TimelinePresentationRoot[],
): SettledPresentationSegment[] {
  const out: SettledPresentationSegment[] = [];
  let processItems: TimelinePresentationItem[] = [];
  let processStepCount = 0;
  let processStartId: string | null = null;

  const flushProcess = () => {
    if (processItems.length === 0 && processStepCount === 0) return;
    out.push({
      kind: 'process',
      id: `process-${processStartId ?? out.length}`,
      items: processItems,
      stepCount: processStepCount,
    });
    processItems = [];
    processStepCount = 0;
    processStartId = null;
  };

  for (const root of roots) {
    if (isStepGroup(root)) {
      if (stepHasVisibleProse(root.items)) {
        flushProcess();
        out.push({ kind: 'final-step', id: root.id, step: root });
      } else {
        if (processStepCount === 0) processStartId = root.id;
        processStepCount += 1;
        processItems.push(...root.items);
      }
      continue;
    }

    const isFinalText =
      root.kind === 'block' &&
      root.block.kind === 'text' &&
      root.block.content.trim().length > 0;

    if (isFinalText) {
      flushProcess();
      out.push({ kind: 'final-item', id: root.block.id, item: root });
    } else {
      if (processItems.length === 0 && processStepCount === 0) {
        processStartId = presentationItemId(root);
      }
      processItems.push(root);
    }
  }
  flushProcess();
  return out;
}

/** Count tools inside presentation items (for settled process summary). */
export function countToolsInPresentationItems(
  items: readonly TimelinePresentationItem[],
): number {
  let n = 0;
  for (const item of items) {
    if (item.kind === 'collapsed_tools') n += item.blocks.length;
    else if (item.kind === 'block' && item.block.kind === 'tool') n += 1;
  }
  return n;
}
