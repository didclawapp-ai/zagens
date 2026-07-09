import type { TurnBlock } from './turnBlockTypes';
import { isPlanTool } from './toolCategories';

/** Max chars for a text block to be treated as a step caption (P2.3). */
export const STEP_CAPTION_MAX_CHARS = 120;

export type ProseCaption = {
  text: string;
  blockId: string;
};

export type ProseConsolidatedBlock =
  | { kind: 'block'; block: TurnBlock }
  | { kind: 'caption'; caption: ProseCaption; block: Extract<TurnBlock, { kind: 'tool' }> };

/**
 * Merge adjacent short prose + following tool into a caption pair for step grouping.
 * Does not mutate underlying blocks — presentation-only.
 */
export function consolidateProseWithTools(blocks: TurnBlock[]): ProseConsolidatedBlock[] {
  const out: ProseConsolidatedBlock[] = [];
  let i = 0;

  while (i < blocks.length) {
    const block = blocks[i];
    const next = blocks[i + 1];

    if (
      block.kind === 'text' &&
      block.content.trim().length > 0 &&
      block.content.trim().length <= STEP_CAPTION_MAX_CHARS &&
      next?.kind === 'tool' &&
      !isPlanTool(next.name)
    ) {
      out.push({
        kind: 'caption',
        caption: { text: block.content.trim(), blockId: block.id },
        block: next,
      });
      i += 2;
      continue;
    }

    out.push({ kind: 'block', block });
    i += 1;
  }

  return out;
}
