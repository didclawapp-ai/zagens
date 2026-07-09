import type { NormalizedStreamEvent } from '../../../api/streamNormalize';
import { appendStreamingTextDelta, mergeAgentMessageSegment } from '../formatAssistantContent';
import {
  appendCappedToolOutput,
  capToolOutputForDisplay,
  mergeStreamingToolOutput,
  stringifyToolInput,
  toolOutputString,
} from '../toolOutput';
import type {
  BoundaryEvent,
  TimelineState,
  TurnBlock,
} from './turnBlockTypes';
import { nextBlockId } from './turnBlockTypes';

const CONCURRENT_TOOL_WINDOW_MS = 50;

const THINKING_BOUNDARIES: ReadonlySet<BoundaryEvent> = new Set([
  'tool_started',
  'tool_completed',
  'message_delta',
  'message_segment',
]);

function proseMatchesTextBlock(existing: string, incoming: string): boolean {
  const cur = existing.trim();
  const next = incoming.trim();
  if (!cur || !next) return false;
  if (cur === next) return true;
  if (next.startsWith(cur) || cur.startsWith(next)) return true;
  if (cur.endsWith(next) || next.endsWith(cur)) return true;
  if (next.length >= 12 && cur.includes(next)) return true;
  if (cur.length >= 12 && next.includes(cur)) return true;
  return false;
}

function recentTextBeforeTrailingTools(
  blocks: TurnBlock[],
): Extract<TurnBlock, { kind: 'text' }> | null {
  let i = blocks.length - 1;
  while (i >= 0 && blocks[i].kind === 'tool') {
    i -= 1;
  }
  const block = blocks[i];
  return block?.kind === 'text' ? block : null;
}

export function createEmptyTimelineState(): TimelineState {
  return {
    blocks: [],
    lastBoundary: null,
    concurrentGroupAnchor: null,
  };
}

function closeOpenThinking(blocks: TurnBlock[]): TurnBlock[] {
  const next = [...blocks];
  for (let i = next.length - 1; i >= 0; i--) {
    const block = next[i];
    if (block.kind === 'thinking' && block.streaming !== false) {
      next[i] = {
        ...block,
        streaming: false,
        status: 'done',
        completedAt: block.completedAt ?? Date.now(),
      };
      break;
    }
    if (block.kind !== 'thinking') break;
  }
  return next;
}

function closeOpenText(blocks: TurnBlock[]): TurnBlock[] {
  const next = [...blocks];
  for (let i = next.length - 1; i >= 0; i--) {
    const block = next[i];
    if (block.kind === 'text' && block.streaming !== false) {
      next[i] = { ...block, streaming: false };
      break;
    }
    if (block.kind !== 'text') break;
  }
  return next;
}

function shouldStartNewThinking(state: TimelineState): boolean {
  return state.lastBoundary != null && THINKING_BOUNDARIES.has(state.lastBoundary);
}

function findRunningToolIndex(blocks: TurnBlock[], toolId?: string | null): number {
  if (toolId) {
    const byId = blocks.findIndex((b) => b.kind === 'tool' && b.id === toolId);
    if (byId >= 0) return byId;
  }
  for (let i = blocks.length - 1; i >= 0; i--) {
    const block = blocks[i];
    if (block.kind === 'tool' && block.status === 'running') return i;
  }
  return -1;
}

function concurrentGroupId(state: TimelineState, now: number): string | undefined {
  if (
    state.concurrentGroupAnchor != null &&
    now - state.concurrentGroupAnchor <= CONCURRENT_TOOL_WINDOW_MS
  ) {
    return `cg-${state.concurrentGroupAnchor}`;
  }
  return undefined;
}

export function finalizeRunningTools(blocks: TurnBlock[]): TurnBlock[] {
  return blocks.map((block) =>
    block.kind === 'tool' && block.status === 'running'
      ? { ...block, status: 'interrupted' as const }
      : block,
  );
}

export function finalizeTimelineBlocks(blocks: TurnBlock[]): TurnBlock[] {
  let next = closeOpenText(closeOpenThinking(blocks));
  next = finalizeRunningTools(next);
  return next;
}

export type TimelineEventContext = {
  currentToolId?: string | null;
};

export function applyTimelineEvent(
  state: TimelineState,
  norm: NormalizedStreamEvent,
  ctx: TimelineEventContext = {},
): TimelineState {
  switch (norm.kind) {
    case 'turn_started':
      return createEmptyTimelineState();

    case 'thinking_delta': {
      let blocks = closeOpenText(state.blocks);
      const last = blocks[blocks.length - 1];
      if (
        last?.kind === 'thinking' &&
        last.streaming !== false &&
        !shouldStartNewThinking(state)
      ) {
        blocks = [
          ...blocks.slice(0, -1),
          {
            ...last,
            text: appendStreamingTextDelta(last.text, norm.content),
            streaming: true,
          },
        ];
      } else {
        blocks = [
          ...blocks,
          {
            kind: 'thinking',
            id: nextBlockId('think'),
            text: norm.content,
            streaming: true,
            status: 'running',
            startedAt: Date.now(),
          },
        ];
      }
      return {
        blocks,
        lastBoundary: 'thinking_delta',
        concurrentGroupAnchor: state.concurrentGroupAnchor,
      };
    }

    case 'message_delta': {
      let blocks = closeOpenThinking(state.blocks);
      const last = blocks[blocks.length - 1];
      if (last?.kind === 'text' && last.streaming !== false) {
        blocks = [
          ...blocks.slice(0, -1),
          {
            ...last,
            content: appendStreamingTextDelta(last.content, norm.content),
            streaming: true,
          },
        ];
      } else {
        blocks = [
          ...blocks,
          {
            kind: 'text',
            id: nextBlockId('text'),
            content: norm.content,
            streaming: true,
          },
        ];
      }
      return {
        blocks,
        lastBoundary: 'message_delta',
        concurrentGroupAnchor: null,
      };
    }

    case 'message_segment': {
      let blocks = closeOpenThinking(state.blocks);
      const last = blocks[blocks.length - 1];
      if (last?.kind === 'text') {
        blocks = [
          ...blocks.slice(0, -1),
          {
            ...last,
            content: mergeAgentMessageSegment(last.content, norm.content),
            streaming: false,
          },
        ];
      } else {
        const prior = recentTextBeforeTrailingTools(blocks);
        if (prior && proseMatchesTextBlock(prior.content, norm.content)) {
          return {
            blocks,
            lastBoundary: 'message_segment',
            concurrentGroupAnchor: null,
          };
        }
        blocks = [
          ...blocks,
          {
            kind: 'text',
            id: nextBlockId('text'),
            content: norm.content,
            streaming: false,
          },
        ];
      }
      return {
        blocks,
        lastBoundary: 'message_segment',
        concurrentGroupAnchor: null,
      };
    }

    case 'tool_started': {
      const now = Date.now();
      let blocks = closeOpenText(closeOpenThinking(state.blocks));
      const existingIdx = blocks.findIndex(
        (block) => block.kind === 'tool' && block.id === norm.id,
      );
      if (existingIdx >= 0) {
        const existing = blocks[existingIdx];
        if (existing.kind === 'tool') {
          blocks = [...blocks];
          blocks[existingIdx] = {
            ...existing,
            name: norm.name || existing.name,
            input: stringifyToolInput(norm.input) || existing.input,
            status: 'running',
          };
        }
      } else {
        blocks = [
          ...blocks,
          {
            kind: 'tool' as const,
            id: norm.id,
            name: norm.name,
            input: stringifyToolInput(norm.input),
            status: 'running' as const,
            concurrentGroupId: concurrentGroupId(state, now),
          },
        ];
      }
      return {
        blocks,
        lastBoundary: 'tool_started',
        concurrentGroupAnchor:
          state.concurrentGroupAnchor != null &&
          now - state.concurrentGroupAnchor <= CONCURRENT_TOOL_WINDOW_MS
            ? state.concurrentGroupAnchor
            : now,
      };
    }

    case 'tool_progress': {
      const idx = findRunningToolIndex(state.blocks, ctx.currentToolId);
      if (idx < 0) return state;
      const tool = state.blocks[idx];
      if (tool.kind !== 'tool') return state;
      const blocks = [...state.blocks];
      blocks[idx] = {
        ...tool,
        output: appendCappedToolOutput(tool.output ?? '', norm.output),
      };
      return { ...state, blocks };
    }

    case 'tool_completed': {
      const outStr = capToolOutputForDisplay(toolOutputString(norm.output));
      let idx = findRunningToolIndex(state.blocks, norm.id);
      if (idx < 0) {
        idx = state.blocks.findIndex(
          (block) => block.kind === 'tool' && block.id === norm.id,
        );
      }
      const blocks = [...state.blocks];
      if (idx < 0) {
        blocks.push({
          kind: 'tool',
          id: norm.id,
          name: 'tool',
          input: '',
          output: outStr,
          status: norm.success ? 'done' : 'error',
        });
      } else {
        const tool = blocks[idx];
        if (tool.kind === 'tool') {
          const merged = capToolOutputForDisplay(
            mergeStreamingToolOutput(tool.output ?? '', outStr || ''),
          );
          blocks[idx] = {
            ...tool,
            output: merged,
            status: norm.success ? 'done' : 'error',
          };
        }
      }
      return {
        blocks,
        lastBoundary: 'tool_completed',
        concurrentGroupAnchor: null,
      };
    }

    case 'turn_completed':
    case 'error':
      return {
        ...state,
        blocks: finalizeTimelineBlocks(state.blocks),
        lastBoundary: 'turn_completed',
        concurrentGroupAnchor: null,
      };

    default:
      return state;
  }
}
