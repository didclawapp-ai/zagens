import type { TurnBlock } from './turnBlockTypes';
import type { ToolCategory } from './toolCategories';

export type TimelineCollapsedCategory = ToolCategory | 'mixed';

export type TimelinePresentationItem =
  | { kind: 'block'; block: TurnBlock }
  | {
      kind: 'collapsed_tools';
      id: string;
      blocks: Extract<TurnBlock, { kind: 'tool' }>[];
      category: TimelineCollapsedCategory;
      /** Completed thinking segments absorbed between tools (P4.6). */
      absorbedThinking?: Extract<TurnBlock, { kind: 'thinking' }>[];
      /**
       * Short lead-in prose absorbed into this activity (thr_ea9c).
       * Used as the activity row label; mid-run captions also soft-split activities.
       */
      absorbedCaptions?: Extract<TurnBlock, { kind: 'text' }>[];
    };

export type TimelineStepGroup = {
  kind: 'step';
  id: string;
  title: string;
  stepIndex: number;
  stepTotal: number;
  items: TimelinePresentationItem[];
};

export type TimelinePresentationRoot = TimelinePresentationItem | TimelineStepGroup;
