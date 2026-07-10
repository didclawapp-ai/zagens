import type { TurnBlock } from './turnBlockTypes';
import {
  collapseNearDuplicateReport,
  isNearDuplicateProse,
} from '../formatAssistantContent';
import { isCollapsibleToolCategory, toolCategory } from './toolCategories';
import {
  groupPresentationIntoSteps,
  deriveStepGroupHintFromBlocks,
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
export { deriveStepGroupHintFromBlocks } from './stepGrouper';

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

/**
 * Collapsible tool that may join an activity row.
 * Includes running tools so live shell thrash collapses (thr_ea9c).
 * done + error share one activity; failure count is surfaced in the summary.
 */
function isActivityTool(
  block: TurnBlock,
): block is Extract<TurnBlock, { kind: 'tool' }> {
  return (
    block.kind === 'tool' &&
    canCollapseCategory(toolCategory(block.name)) &&
    (block.status === 'done' || block.status === 'error' || block.status === 'running')
  );
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

type LeadingAbsorbed = {
  thinking: Extract<TurnBlock, { kind: 'thinking' }>[];
  captions: Extract<TurnBlock, { kind: 'text' }>[];
};

/** Thinking/captions skipped just before a tool run — attach into the activity. */
function collectLeadingAbsorbed(
  blocks: readonly TurnBlock[],
  toolIndex: number,
): LeadingAbsorbed {
  const thinking: Extract<TurnBlock, { kind: 'thinking' }>[] = [];
  const captions: Extract<TurnBlock, { kind: 'text' }>[] = [];
  let k = toolIndex - 1;
  while (k >= 0) {
    const prev = blocks[k];
    if (isAbsorbedThinkingAt(blocks, k) && prev.kind === 'thinking') {
      thinking.unshift(prev);
      k -= 1;
      continue;
    }
    if (isAbsorbedCaptionAt(blocks, k) && prev.kind === 'text') {
      captions.unshift(prev);
      k -= 1;
      continue;
    }
    break;
  }
  return { thinking, captions };
}

function asCollapsibleToolBlock(
  item: TimelinePresentationItem,
): Extract<TurnBlock, { kind: 'tool' }> | null {
  if (item.kind === 'collapsed_tools' && item.blocks.length > 0) {
    return item.blocks[0];
  }
  if (item.kind === 'block' && isActivityTool(item.block)) {
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

function itemAbsorbedCaptions(
  item: TimelinePresentationItem,
): Extract<TurnBlock, { kind: 'text' }>[] {
  return item.kind === 'collapsed_tools' ? (item.absorbedCaptions ?? []) : [];
}

function collapsedActivityItem(
  blocks: Extract<TurnBlock, { kind: 'tool' }>[],
  absorbedThinking: Extract<TurnBlock, { kind: 'thinking' }>[],
  absorbedCaptions: Extract<TurnBlock, { kind: 'text' }>[],
): TimelinePresentationItem {
  return {
    kind: 'collapsed_tools',
    id: `collapsed-${blocks[0].id}`,
    blocks,
    category: resolveCollapsedCategory(blocks),
    ...(absorbedThinking.length > 0 ? { absorbedThinking } : {}),
    ...(absorbedCaptions.length > 0 ? { absorbedCaptions } : {}),
  };
}

/**
 * Merge adjacent collapsed bundles and lone collapsible tool rows (thr_82ac).
 * Does not merge across a caption-marked phase boundary (thr_ea9c).
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

    const prev = out[out.length - 1];
    const prevTool = prev ? asCollapsibleToolBlock(prev) : null;
    const incomingCaptions = itemAbsorbedCaptions(item);

    // Caption on the incoming row marks a narrative phase — keep separate.
    if (prev && prevTool && incomingCaptions.length === 0) {
      const blocks = [...itemToolBlocks(prev), ...itemToolBlocks(item)];
      const absorbed = [...itemAbsorbedThinking(prev), ...itemAbsorbedThinking(item)];
      const captions = [...itemAbsorbedCaptions(prev), ...incomingCaptions];
      out[out.length - 1] = collapsedActivityItem(blocks, absorbed, captions);
      continue;
    }

    // Promote a lone collapsible tool to a one-tool activity row for consistent chrome.
    if (item.kind === 'block' && item.block.kind === 'tool') {
      out.push(collapsedActivityItem([item.block], [], []));
      continue;
    }

    out.push(item);
  }

  return out;
}

/**
 * Bundle tool activity for scanability (P2.2 / P4 / P4.6 / thr_ea9c).
 *
 * - Short prose and **completed thinking** between tools are absorbed (not top-level rows).
 * - Mid-run **captions soft-split** activities so long turns keep phase labels.
 * - explore / write / shell / plan / office / workflow / agent may merge across thinking gaps
 *   (category `mixed` when kinds differ).
 * - done + error + running share one activity; failure/running counts surface in the summary.
 * - Long final prose stays expanded.
 * - Adjacent activity rows merge unless a caption marks a new phase.
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

    if (!isActivityTool(block)) {
      items.push({ kind: 'block', block });
      i += 1;
      continue;
    }

    const leading = collectLeadingAbsorbed(blocks, i);
    const run: Extract<TurnBlock, { kind: 'tool' }>[] = [block];
    const absorbedThinking = [...leading.thinking];
    const absorbedCaptions = [...leading.captions];
    let j = i + 1;

    while (j < blocks.length) {
      const next = blocks[j];
      // Caption after tools already collected → soft phase boundary (thr_ea9c).
      if (isAbsorbedCaptionAt(blocks, j)) {
        break;
      }
      if (isAbsorbedThinkingAt(blocks, j)) {
        if (next.kind === 'thinking') absorbedThinking.push(next);
        j += 1;
        continue;
      }
      if (isActivityTool(next)) {
        run.push(next);
        j += 1;
        continue;
      }
      break;
    }

    // Collapse multi-tool runs, single collapsible tools, or runs that absorbed reasoning/captions.
    const shouldCollapse =
      run.length >= MIN_COLLAPSE_COUNT ||
      run.length === 1 ||
      absorbedThinking.length > 0 ||
      absorbedCaptions.length > 0;

    if (shouldCollapse && run.length >= 1) {
      items.push(collapsedActivityItem(run, absorbedThinking, absorbedCaptions));
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
  const stepHint = options.stepHint ?? deriveStepGroupHintFromBlocks(deduped);
  return groupPresentationIntoSteps(deduped, prepareTimelinePresentation, stepHint);
}
