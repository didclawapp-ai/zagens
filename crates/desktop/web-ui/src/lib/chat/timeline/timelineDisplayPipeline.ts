import type { TurnBlock } from './turnBlockTypes';
import { isCollapsibleToolCategory, toolCategory } from './toolCategories';
import {
  groupPresentationIntoSteps,
  type StepGroupHint,
} from './stepGrouper';
import { STEP_CAPTION_MAX_CHARS } from './proseConsolidation';
import type {
  TimelineCollapsedCategory,
  TimelinePresentationItem,
  TimelinePresentationRoot,
} from './timelinePresentationTypes';

export type {
  TimelineCollapsedCategory,
  TimelinePresentationItem,
  TimelinePresentationRoot,
  TimelineStepGroup,
} from './timelinePresentationTypes';
export type { StepGroupHint };

/** Collapse when an activity run reaches this tool count (P4.2). */
export const MIN_COLLAPSE_COUNT = 2;

/**
 * Categories that participate in activity bundling (P4.3 / P4.6 / office).
 * Cross-category runs become `mixed` when only absorbed thinking/prose separates them.
 */
export function canCollapseCategory(
  category: Parameters<typeof isCollapsibleToolCategory>[0],
): boolean {
  return isCollapsibleToolCategory(category);
}

function isShortCaptionText(block: TurnBlock): boolean {
  return (
    block.kind === 'text' &&
    block.content.trim().length > 0 &&
    block.content.trim().length <= STEP_CAPTION_MAX_CHARS
  );
}

function isCompletedThinking(block: TurnBlock): block is Extract<TurnBlock, { kind: 'thinking' }> {
  return (
    block.kind === 'thinking' &&
    block.streaming !== true &&
    block.status !== 'running'
  );
}

/** Look ahead: eventually hits a tool, only crossing absorbable gaps. */
function eventuallyReachesTool(blocks: readonly TurnBlock[], from: number): boolean {
  for (let j = from; j < blocks.length; j++) {
    const next = blocks[j];
    if (next.kind === 'tool') return true;
    if (isShortCaptionText(next) || isCompletedThinking(next)) continue;
    return false;
  }
  return false;
}

/** Short prose lead-in before tools — do not render as its own row. */
function isAbsorbedCaptionAt(blocks: readonly TurnBlock[], index: number): boolean {
  if (!isShortCaptionText(blocks[index])) return false;
  return eventuallyReachesTool(blocks, index + 1);
}

/**
 * Completed thinking that only bridges toward upcoming tools (P4.6).
 * Fixes 推理→执行→推理→执行 fragmentation: thinking is folded into the activity.
 * Streaming / in-progress thinking stays visible.
 */
function isAbsorbedThinkingAt(blocks: readonly TurnBlock[], index: number): boolean {
  if (!isCompletedThinking(blocks[index])) return false;
  return eventuallyReachesTool(blocks, index + 1);
}

function isAbsorbedGapAt(blocks: readonly TurnBlock[], index: number): boolean {
  return isAbsorbedCaptionAt(blocks, index) || isAbsorbedThinkingAt(blocks, index);
}

function collapseBucket(
  block: Extract<TurnBlock, { kind: 'tool' }>,
): 'done' | 'error' | null {
  if (block.status === 'done') return 'done';
  if (block.status === 'error') return 'error';
  return null;
}

function resolveCollapsedCategory(
  tools: Extract<TurnBlock, { kind: 'tool' }>[],
): TimelineCollapsedCategory {
  if (tools.length === 0) return 'other';
  const first = toolCategory(tools[0].name);
  for (let i = 1; i < tools.length; i++) {
    if (toolCategory(tools[i].name) !== first) return 'mixed';
  }
  return first;
}

/** Thinking/captions skipped just before a tool run — attach into the activity. */
function collectLeadingAbsorbedThinking(
  blocks: readonly TurnBlock[],
  toolIndex: number,
): Extract<TurnBlock, { kind: 'thinking' }>[] {
  const out: Extract<TurnBlock, { kind: 'thinking' }>[] = [];
  let k = toolIndex - 1;
  while (k >= 0) {
    const prev = blocks[k];
    if (isAbsorbedThinkingAt(blocks, k) && prev.kind === 'thinking') {
      out.unshift(prev);
      k -= 1;
      continue;
    }
    if (isAbsorbedCaptionAt(blocks, k)) {
      k -= 1;
      continue;
    }
    break;
  }
  return out;
}

/**
 * Bundle tool activity for scanability (P2.2 / P4 / P4.6).
 *
 * - Short prose and **completed thinking** between tools are absorbed (not top-level rows).
 * - explore / write / shell / plan / office / workflow / agent may merge across those gaps into one activity
 *   (category `mixed` when kinds differ).
 * - done vs error stay in separate bundles.
 * - Running tools and long final prose stay expanded.
 */
export function prepareTimelinePresentation(blocks: TurnBlock[]): TimelinePresentationItem[] {
  const items: TimelinePresentationItem[] = [];
  let i = 0;

  while (i < blocks.length) {
    const block = blocks[i];

    if (isAbsorbedGapAt(blocks, i)) {
      i += 1;
      continue;
    }

    if (block.kind !== 'tool') {
      items.push({ kind: 'block', block });
      i += 1;
      continue;
    }

    const bucket = collapseBucket(block);
    if (!canCollapseCategory(toolCategory(block.name)) || bucket == null) {
      items.push({ kind: 'block', block });
      i += 1;
      continue;
    }

    const run: Extract<TurnBlock, { kind: 'tool' }>[] = [block];
    const absorbedThinking = collectLeadingAbsorbedThinking(blocks, i);
    let j = i + 1;

    while (j < blocks.length) {
      const next = blocks[j];
      if (isAbsorbedCaptionAt(blocks, j)) {
        j += 1;
        continue;
      }
      if (isAbsorbedThinkingAt(blocks, j)) {
        if (next.kind === 'thinking') absorbedThinking.push(next);
        j += 1;
        continue;
      }
      if (
        next.kind === 'tool' &&
        canCollapseCategory(toolCategory(next.name)) &&
        collapseBucket(next) === bucket
      ) {
        run.push(next);
        j += 1;
        continue;
      }
      break;
    }

    const category = resolveCollapsedCategory(run);
    // Collapse multi-tool runs, or any run that absorbed reasoning (kills 推理↔执行 flicker).
    const shouldCollapse = run.length >= MIN_COLLAPSE_COUNT || absorbedThinking.length > 0;

    if (shouldCollapse && run.length >= 1) {
      items.push({
        kind: 'collapsed_tools',
        id: `collapsed-${run[0].id}`,
        blocks: run,
        category,
        ...(absorbedThinking.length > 0 ? { absorbedThinking } : {}),
      });
    } else {
      for (const tool of run) {
        items.push({ kind: 'block', block: tool });
      }
    }
    i = j;
  }

  return items;
}

export type BuildTimelinePresentationOptions = {
  stepGrouping?: boolean;
  stepHint?: StepGroupHint;
};

/** Full display pipeline: optional step groups + activity collapse (P2 / P4 / P4.6). */
export function buildTimelinePresentation(
  blocks: TurnBlock[],
  options: BuildTimelinePresentationOptions = {},
): TimelinePresentationRoot[] {
  if (!options.stepGrouping || blocks.length < 8) {
    return prepareTimelinePresentation(blocks);
  }
  return groupPresentationIntoSteps(blocks, prepareTimelinePresentation, options.stepHint);
}
