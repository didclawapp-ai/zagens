import type { TurnBlock } from './turnBlockTypes';
import type { ToolCategory } from './toolCategories';

export type TimelinePresentationItem =
  | { kind: 'block'; block: TurnBlock }
  | {
      kind: 'collapsed_tools';
      id: string;
      blocks: Extract<TurnBlock, { kind: 'tool' }>[];
      category: ToolCategory;
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
