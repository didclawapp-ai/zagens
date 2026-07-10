import type { TurnBlock } from './turnBlockTypes';
import {
  collapseNearDuplicateReport,
  isNearDuplicateProse,
} from '../formatAssistantContent';
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

function asCollapsibleToolBlock(
  item: TimelinePresentationItem,
): Extract<TurnBlock, { kind: 'tool' }> | null {
  if (item.kind === 'collapsed_tools' && item.blocks.length > 0) {
    return item.blocks[0];
  }
  if (
    item.kind === 'block' &&
    item.block.kind === 'tool' &&
    canCollapseCategory(toolCategory(item.block.name)) &&
    collapseBucket(item.block) != null
  ) {
    return item.block;
  }
  return null;
}

function itemToolBlocks(
  item: TimelinePresentationItem,
): Extract<TurnBlock, { kind: 'tool' }>[] {
  if (item.kind === 'collapsed_tools') return item.blocks;
  if (item.kind === 'block' && item.block.kind === 'tool') return [item.block];
  return [];
}

function itemAbsorbedThinking(
  item: TimelinePresentationItem,
): Extract<TurnBlock, { kind: 'thinking' }>[] {
  return item.kind === 'collapsed_tools' ? (item.absorbedThinking ?? []) : [];
}

/**
 * Merge adjacent collapsed bundles and lone collapsible tool rows (thr_82ac).
 * Keeps done/error buckets separate; category becomes `mixed` when kinds differ.
 */
export function mergeAdjacentActivityItems(
  items: TimelinePresentationItem[],
): TimelinePresentationItem[] {
  const out: TimelinePresentationItem[] = [];

  for (const item of items) {
    const tool = asCollapsibleToolBlock(item);
    if (!tool) {
      out.push(item);
      continue;
    }

    const bucket = collapseBucket(tool);
    const prev = out[out.length - 1];
    const prevTool = prev ? asCollapsibleToolBlock(prev) : null;
    const prevBucket = prevTool ? collapseBucket(prevTool) : null;

    if (prev && prevTool && prevBucket != null && prevBucket === bucket) {
      const blocks = [...itemToolBlocks(prev), ...itemToolBlocks(item)];
      const absorbed = [...itemAbsorbedThinking(prev), ...itemAbsorbedThinking(item)];
      out[out.length - 1] = {
        kind: 'collapsed_tools',
        id: `collapsed-${blocks[0].id}`,
        blocks,
        category: resolveCollapsedCategory(blocks),
        ...(absorbed.length > 0 ? { absorbedThinking: absorbed } : {}),
      };
      continue;
    }

    // Promote a lone collapsible tool to a one-tool activity row for consistent chrome.
    if (item.kind === 'block' && item.block.kind === 'tool') {
      out.push({
        kind: 'collapsed_tools',
        id: `collapsed-${item.block.id}`,
        blocks: [item.block],
        category: resolveCollapsedCategory([item.block]),
      });
      continue;
    }

    out.push(item);
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
 * - Adjacent activity rows (incl. single tools) are merged (audit-turn polish).
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
    // Collapse multi-tool runs, single collapsible tools, or runs that absorbed reasoning.
    const shouldCollapse =
      run.length >= MIN_COLLAPSE_COUNT ||
      run.length === 1 ||
      absorbedThinking.length > 0;

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

  return mergeAdjacentActivityItems(items);
}

export type BuildTimelinePresentationOptions = {
  stepGrouping?: boolean;
  stepHint?: StepGroupHint;
};

/**
 * Drop rewritten final-report duplicates (in-block halves or adjacent text blocks)
 * before step grouping / activity collapse.
 */
export function dedupeTimelineProseBlocks(blocks: TurnBlock[]): TurnBlock[] {
  const out: TurnBlock[] = [];
  for (const block of blocks) {
    if (block.kind !== 'text') {
      out.push(block);
      continue;
    }
    const content = collapseNearDuplicateReport(block.content);
    const normalized =
      content === block.content ? block : { ...block, content };
    const prev = out[out.length - 1];
    if (
      prev?.kind === 'text' &&
      isNearDuplicateProse(prev.content, normalized.content)
    ) {
      if (normalized.content.length > prev.content.length) {
        out[out.length - 1] = { ...prev, content: normalized.content };
      }
      continue;
    }
    out.push(normalized);
  }
  return out;
}

/** Full display pipeline: optional step groups + activity collapse (P2 / P4 / P4.6). */
export function buildTimelinePresentation(
  blocks: TurnBlock[],
  options: BuildTimelinePresentationOptions = {},
): TimelinePresentationRoot[] {
  const deduped = dedupeTimelineProseBlocks(blocks);
  if (!options.stepGrouping || deduped.length < 8) {
    return prepareTimelinePresentation(deduped);
  }
  return groupPresentationIntoSteps(deduped, prepareTimelinePresentation, options.stepHint);
}
