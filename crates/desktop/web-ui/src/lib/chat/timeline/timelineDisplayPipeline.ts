import type { TurnBlock } from './turnBlockTypes';
import { toolCategory, type ToolCategory } from './toolCategories';
import {
  groupPresentationIntoSteps,
  type StepGroupHint,
} from './stepGrouper';
import type {
  TimelinePresentationItem,
  TimelinePresentationRoot,
} from './timelinePresentationTypes';

export type {
  TimelinePresentationItem,
  TimelinePresentationRoot,
  TimelineStepGroup,
} from './timelinePresentationTypes';
export type { StepGroupHint };

const MIN_COLLAPSE_COUNT = 3;

function isDoneTool(block: Extract<TurnBlock, { kind: 'tool' }>): boolean {
  return block.status === 'done' || block.status === 'error';
}

function canCollapseCategory(category: ToolCategory): boolean {
  return category === 'explore' || category === 'write';
}

/**
 * Collapse long runs of homogeneous completed tools for scanability (P2.2).
 * Running tools and thinking/text blocks are always emitted individually.
 */
export function prepareTimelinePresentation(blocks: TurnBlock[]): TimelinePresentationItem[] {
  const items: TimelinePresentationItem[] = [];
  let i = 0;

  while (i < blocks.length) {
    const block = blocks[i];
    if (block.kind !== 'tool') {
      items.push({ kind: 'block', block });
      i += 1;
      continue;
    }

    const category = toolCategory(block.name);
    if (!canCollapseCategory(category) || !isDoneTool(block)) {
      items.push({ kind: 'block', block });
      i += 1;
      continue;
    }

    const run: Extract<TurnBlock, { kind: 'tool' }>[] = [block];
    let j = i + 1;
    while (j < blocks.length) {
      const next = blocks[j];
      if (
        next.kind === 'tool' &&
        toolCategory(next.name) === category &&
        isDoneTool(next)
      ) {
        run.push(next);
        j += 1;
        continue;
      }
      break;
    }

    if (run.length >= MIN_COLLAPSE_COUNT) {
      items.push({
        kind: 'collapsed_tools',
        id: `collapsed-${run[0].id}`,
        blocks: run,
        category,
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

/** Full display pipeline: optional step groups + tool-run collapse (P2). */
export function buildTimelinePresentation(
  blocks: TurnBlock[],
  options: BuildTimelinePresentationOptions = {},
): TimelinePresentationRoot[] {
  if (!options.stepGrouping || blocks.length < 8) {
    return prepareTimelinePresentation(blocks);
  }
  return groupPresentationIntoSteps(blocks, prepareTimelinePresentation, options.stepHint);
}
