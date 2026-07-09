import type { TurnBlock } from './turnBlockTypes';
import { isPlanTool } from './toolCategories';
import { STEP_CAPTION_MAX_CHARS } from './proseConsolidation';
import type {
  TimelinePresentationItem,
  TimelinePresentationRoot,
  TimelineStepGroup,
} from './timelinePresentationTypes';

export type StepGroupHint = {
  inProgressChecklistId?: number | null;
  checklistItems?: ReadonlyArray<{ id: number; content: string }>;
};

export type { TimelinePresentationRoot, TimelineStepGroup };

function checklistTitle(hint: StepGroupHint | undefined, stepIndex: number): string | null {
  const id = hint?.inProgressChecklistId;
  if (id == null || !hint?.checklistItems?.length) return null;
  const item = hint.checklistItems.find((c) => c.id === id);
  if (!item?.content.trim()) return null;
  return item.content.trim();
}

function titleFromTextBlock(block: Extract<TurnBlock, { kind: 'text' }>): string | null {
  const text = block.content.trim();
  if (!text) return null;
  if (text.length <= STEP_CAPTION_MAX_CHARS) return text;
  return `${text.slice(0, STEP_CAPTION_MAX_CHARS).trimEnd()}…`;
}

function splitStepSegments(blocks: TurnBlock[]): TurnBlock[][] {
  const segments: TurnBlock[][] = [];
  let current: TurnBlock[] = [];

  const flush = () => {
    if (current.length > 0) {
      segments.push(current);
      current = [];
    }
  };

  for (const block of blocks) {
    // Long prose = phase boundary (final report / major narrative).
    // Plan tools no longer split steps — they collapse inside the segment (P4.5).
    if (block.kind === 'text' && block.content.trim().length > STEP_CAPTION_MAX_CHARS) {
      flush();
      segments.push([block]);
      continue;
    }
    current.push(block);
  }
  flush();
  return segments;
}

function segmentTitle(
  segment: TurnBlock[],
  stepIndex: number,
  hint: StepGroupHint | undefined,
): string {
  const fromChecklist = checklistTitle(hint, stepIndex);
  if (fromChecklist) return fromChecklist;

  const prose = segment.find((b) => b.kind === 'text');
  if (prose?.kind === 'text') {
    const titled = titleFromTextBlock(prose);
    if (titled) return titled;
  }

  const plan = segment.find((b) => b.kind === 'tool' && isPlanTool(b.name));
  if (plan?.kind === 'tool') {
    return plan.name;
  }

  return '';
}

function presentationItemsFromSegment(
  segment: TurnBlock[],
  prepareItems: (blocks: TurnBlock[]) => TimelinePresentationItem[],
  titleText?: string,
): TimelinePresentationItem[] {
  let working = segment;
  const title = titleText?.trim();
  if (title) {
    let removed = false;
    working = segment.filter((block) => {
      if (!removed && block.kind === 'text' && block.content.trim() === title) {
        removed = true;
        return false;
      }
      return true;
    });
  }

  // Drop short lead-in prose that only captions the following tool run (P4.2).
  const stripped: TurnBlock[] = [];
  for (let i = 0; i < working.length; i++) {
    const block = working[i];
    const next = working[i + 1];
    if (
      block.kind === 'text' &&
      block.content.trim().length > 0 &&
      block.content.trim().length <= STEP_CAPTION_MAX_CHARS &&
      next?.kind === 'tool'
    ) {
      continue;
    }
    stripped.push(block);
  }

  return prepareItems(stripped);
}

/**
 * Group presentation items into step cards (P2.2).
 * Uses long text / plan tools as boundaries; optional checklist hint for titles.
 */
export function groupPresentationIntoSteps(
  blocks: TurnBlock[],
  prepareItems: (blocks: TurnBlock[]) => TimelinePresentationItem[],
  hint?: StepGroupHint,
): TimelinePresentationRoot[] {
  const segments = splitStepSegments(blocks);
  if (segments.length <= 1) {
    return prepareItems(blocks);
  }

  const roots: TimelineStepGroup[] = [];
  let stepIndex = 0;

  for (const segment of segments) {
    const title = segmentTitle(segment, stepIndex + 1, hint);
    const items = presentationItemsFromSegment(segment, prepareItems, title);
    if (items.length === 0) continue;
    stepIndex += 1;
    roots.push({
      kind: 'step',
      id: `step-${stepIndex}-${segment[0]?.id ?? 'x'}`,
      title,
      stepIndex,
      stepTotal: segments.length,
      items,
    });
  }

  if (roots.length === 0) {
    return prepareItems(blocks);
  }

  return roots.map((group) => ({
    ...group,
    stepTotal: roots.length,
  }));
}
