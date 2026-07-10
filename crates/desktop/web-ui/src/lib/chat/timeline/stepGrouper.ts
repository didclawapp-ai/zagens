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

/**
 * Derive step titles from checklist_* tool inputs in the turn (no panel wiring).
 * Uses the latest full `todos` write + latest in_progress id.
 */
export function deriveStepGroupHintFromBlocks(
  blocks: readonly TurnBlock[],
): StepGroupHint | undefined {
  let checklistItems: { id: number; content: string }[] = [];
  let inProgressChecklistId: number | null = null;

  for (const block of blocks) {
    if (block.kind !== 'tool' || !isPlanTool(block.name)) continue;
    let input: Record<string, unknown>;
    try {
      const parsed = JSON.parse(block.input || '{}') as unknown;
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) continue;
      input = parsed as Record<string, unknown>;
    } catch {
      continue;
    }

    const todos = input.todos;
    if (Array.isArray(todos) && todos.length > 0) {
      const next: { id: number; content: string }[] = [];
      for (let i = 0; i < todos.length; i++) {
        const row = todos[i];
        if (!row || typeof row !== 'object' || Array.isArray(row)) continue;
        const t = row as Record<string, unknown>;
        const content = typeof t.content === 'string' ? t.content.trim() : '';
        if (!content) continue;
        const id = typeof t.id === 'number' && Number.isFinite(t.id) ? t.id : i + 1;
        next.push({ id, content });
        if (t.status === 'in_progress') inProgressChecklistId = id;
      }
      if (next.length > 0) checklistItems = next;
      continue;
    }

    if (typeof input.id === 'number' && Number.isFinite(input.id)) {
      if (input.status === 'in_progress') inProgressChecklistId = input.id;
      else if (input.status === 'completed' && inProgressChecklistId === input.id) {
        inProgressChecklistId = null;
      }
    }
  }

  if (checklistItems.length === 0) return undefined;
  return { checklistItems, inProgressChecklistId };
}

function checklistTitle(hint: StepGroupHint | undefined, stepIndex: number): string | null {
  if (!hint?.checklistItems?.length) return null;
  // Positional match for tool-only steps (step 1 → first todo, …).
  const byPos = hint.checklistItems[stepIndex - 1];
  if (byPos?.content.trim()) return byPos.content.trim();
  const id = hint.inProgressChecklistId;
  if (id == null) return null;
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
  // Caption / report prose first — checklist must not override final-report titles.
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

  const fromChecklist = checklistTitle(hint, stepIndex);
  if (fromChecklist) return shortenStepTitle(fromChecklist);

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

  // Keep mid-step captions for activity soft-splits + labels (thr_ea9c).
  // prepareTimelinePresentation absorbs them into collapsed_tools.absorbedCaptions.
  return prepareItems(working);
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
