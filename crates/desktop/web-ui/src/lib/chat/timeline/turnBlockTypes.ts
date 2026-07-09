export type TurnBlockKind = 'thinking' | 'tool' | 'text';

export type BlockLifecycleStatus =
  | 'running'
  | 'done'
  | 'error'
  | 'timeout'
  | 'interrupted';

export type TurnBlock =
  | {
      kind: 'thinking';
      id: string;
      text: string;
      streaming?: boolean;
      status?: BlockLifecycleStatus;
      startedAt?: number;
      completedAt?: number;
    }
  | {
      kind: 'tool';
      id: string;
      name: string;
      input: string;
      output?: string;
      status: BlockLifecycleStatus;
      concurrentGroupId?: string;
      itemId?: string;
    }
  | {
      kind: 'text';
      id: string;
      content: string;
      streaming?: boolean;
      itemId?: string;
    };

export type BoundaryEvent =
  | 'turn_started'
  | 'thinking_delta'
  | 'tool_started'
  | 'tool_completed'
  | 'message_delta'
  | 'message_segment'
  | 'turn_completed';

export type TimelineState = {
  blocks: TurnBlock[];
  lastBoundary: BoundaryEvent | null;
  /** Monotonic ms timestamp for concurrent tool grouping. */
  concurrentGroupAnchor: number | null;
};

let blockSeq = 0;

export function nextBlockId(prefix: string): string {
  blockSeq += 1;
  return `${prefix}-${blockSeq}`;
}

export function resetBlockIdCounter(): void {
  blockSeq = 0;
}
