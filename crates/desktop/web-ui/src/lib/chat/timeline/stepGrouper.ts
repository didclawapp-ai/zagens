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

/** Max chars for a step card title (scanability; thr_82ac long reports). */
export const STEP_TITLE_MAX_CHARS = 72;

function checklistTitle(hint: StepGroupHint | undefined, stepIndex: number): string | null {
  const id = hint?.inProgressChecklistId;
  if (id == null || !hint?.checklistItems?.length) return null;
  const item = hint.checklistItems.find((c) => c.id === id);
  if (!item?.content.trim()) return null;
  return item.content.trim();
}

/** Prefer leading heading / first sentence over a long truncated dump. */
export function shortenStepTitle(text: string, maxChars = STEP_TITLE_MAX_CHARS): string {
  let s = text.replace(/\r\n/g, '\n').trim();
  if (!s) return '';

  // Only treat a heading as the title when it opens the block (not mid-report).
  const leadingHeading = s.match(/^#{1,6}\s+(.+?)(?:\n|$)/);
  if (leadingHeading?.[1]?.trim()) {
    s = leadingHeading[1].trim();
  } else {
    const firstLine = s.split('\n').find((line) => line.trim()) ?? s;
    s = firstLine.trim();
    // Prefer the first sentence (CJK often has no space after 。！？).
    const cjkSentence = s.match(/^(.+?[。！？])/);
    if (cjkSentence?.[1] && cjkSentence[1].length >= 4) {
      s = cjkSentence[1];
    } else {
      const latinSentence = s.match(/^(.+?[.!?])(?:\s|$)/);
      if (latinSentence?.[1] && latinSentence[1].length >= 8) {
        s = latinSentence[1].trim();
      }
    }
  }

  if (s.length <= maxChars) return s;
  return `${s.slice(0, maxChars).trimEnd()}…`;
}

function titleFromTextBlock(block: Extract<TurnBlock, { kind: 'text' }>): string | null {
  const text = block.content.trim();
  if (!text) return null;
  return shortenStepTitle(text);
}

/** Trailing completed-thinking after a long final report — not a real phase. */
function isThinkingOnlySegment(segment: TurnBlock[]): boolean {
  return (
    segment.length > 0 &&
    segment.every(
      (b) =>
        b.kind === 'thinking' &&
        b.streaming !== true &&
        b.status !== 'running',
    )
  );
}

/**
 * Fold trailing thinking-only segments into the previous phase.
 * Fixes empty "步骤 N/N" cards that only contain collapsed 推理 after a final report.
 */
function coalesceTrailingThinkingSegments(segments: TurnBlock[][]): TurnBlock[][] {
  if (segments.length < 2) return segments;
  const out = segments.map((s) => [...s]);
  while (out.length >= 2 && isThinkingOnlySegment(out[out.length - 1])) {
    const trailing = out.pop()!;
    out[out.length - 1] = [...out[out.length - 1], ...trailing];
  }
  return out;
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
  return coalesceTrailingThinkingSegments(segments);
}

function segmentTitle(
  segment: TurnBlock[],
  stepIndex: number,
  hint: StepGroupHint | undefined,
): string {
  const fromChecklist = checklistTitle(hint, stepIndex);
  if (fromChecklist) return shortenStepTitle(fromChecklist);

  // Prefer a short caption over a long final-report body when both exist.
  const shortProse = segment.find(
    (b) => b.kind === 'text' && b.content.trim().length <= STEP_CAPTION_MAX_CHARS,
  );
  if (shortProse?.kind === 'text') {
    const titled = titleFromTextBlock(shortProse);
    if (titled) return titled;
  }

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
      if (removed || block.kind !== 'text') return true;
      const raw = block.content.trim();
      // Exact match (short caption used as title) or title derived from this block.
      if (raw === title || shortenStepTitle(raw) === title) {
        // Only strip short captions from the body — keep long final reports visible.
        if (raw.length <= STEP_CAPTION_MAX_CHARS) {
          removed = true;
          return false;
        }
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

function isThinkingOnlyPresentation(items: TimelinePresentationItem[]): boolean {
  if (items.length === 0) return false;
  return items.every(
    (item) =>
      item.kind === 'block' &&
      item.block.kind === 'thinking' &&
      item.block.streaming !== true &&
      item.block.status !== 'running',
  );
}

/**
 * Group presentation items into step cards (P2.2).
 * Uses long text as phase boundaries; optional checklist hint for titles.
 * Trailing thinking-only phases fold into the previous step (no empty 步骤 N/N).
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

    // Safety net: untitled thinking-only cards merge into the previous step.
    if (!title.trim() && isThinkingOnlyPresentation(items) && roots.length > 0) {
      const prev = roots[roots.length - 1];
      roots[roots.length - 1] = {
        ...prev,
        items: [...prev.items, ...items],
      };
      continue;
    }

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
