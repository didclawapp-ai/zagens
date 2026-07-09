import { normalizeDesktopStreamEvent, type NormalizedStreamEvent } from '../../../api/streamNormalize';
import type { TurnItemRecord } from '../../../api/runtimeTypes';
import type { TurnBlock } from './turnBlockTypes';
import { nextBlockId, resetBlockIdCounter } from './turnBlockTypes';
import {
  applyTimelineEvent,
  createEmptyTimelineState,
  finalizeTimelineBlocks,
} from './turnTimelineReducer';
import { stringifyToolInput } from '../toolOutput';

export type RawThreadEvent = { event: string; data: string };

export type AssistantBlocksReplay = {
  blocks: TurnBlock[];
  /** True when thinking likely existed live but events/items did not persist it. */
  thinkingIncomplete?: boolean;
};

/** Resolve tool name from durable item metadata (P4.1). */
export function toolNameFromItem(
  kind: TurnItemRecord['kind'],
  metadata: unknown,
  summary?: string,
): string {
  if (metadata && typeof metadata === 'object') {
    const m = metadata as Record<string, unknown>;
    const canonical = m.canonical_tool;
    if (typeof canonical === 'string' && canonical.trim()) return canonical.trim();
    const toolName = m.tool_name;
    if (typeof toolName === 'string' && toolName.trim()) return toolName.trim();
    const tool = m.tool;
    if (tool && typeof tool === 'object') {
      const name = (tool as Record<string, unknown>).name;
      if (typeof name === 'string' && name.trim()) return name.trim();
    }
    const name = m.name;
    if (typeof name === 'string' && name.trim()) return name.trim();
  }
  const fromSummary = String(summary ?? '').match(/^([A-Za-z][A-Za-z0-9_]*)\s*:/);
  if (fromSummary) return fromSummary[1];
  if (kind === 'file_change') return 'write_file';
  if (kind === 'command_execution') return 'exec_shell';
  return kind;
}

function toolInputFromItem(metadata: unknown): string {
  if (!metadata || typeof metadata !== 'object') return '';
  const m = metadata as Record<string, unknown>;
  if ('tool_input' in m) {
    return stringifyToolInput(m.tool_input);
  }
  const tool = m.tool;
  if (tool && typeof tool === 'object' && 'input' in tool) {
    return stringifyToolInput((tool as Record<string, unknown>).input);
  }
  return '';
}

function toolIdFromItem(item: TurnItemRecord, metadata: unknown): string {
  if (metadata && typeof metadata === 'object') {
    const m = metadata as Record<string, unknown>;
    const engineId = m.engine_tool_id;
    if (typeof engineId === 'string' && engineId.trim()) return engineId.trim();
    const tool = m.tool;
    if (tool && typeof tool === 'object') {
      const id = (tool as Record<string, unknown>).id;
      if (typeof id === 'string' && id.trim()) return id;
    }
  }
  return item.id;
}

function itemStatus(
  status: TurnItemRecord['status'],
): Extract<TurnBlock, { kind: 'tool' }>['status'] {
  if (status === 'failed' || status === 'interrupted') return 'error';
  if (status === 'in_progress' || status === 'queued') return 'running';
  return 'done';
}

/** Replay one turn's assistant blocks from durable thread items (interleaved order). */
export function turnTimelineReplayFromThreadItems(
  items: readonly TurnItemRecord[],
  turnId: string,
): TurnBlock[] {
  resetBlockIdCounter();
  const blocks: TurnBlock[] = [];
  let currentToolId: string | null = null;

  const turnItems = items.filter((item) => item.turn_id === turnId);

  for (const item of turnItems) {
    const detail = (item.detail ?? '').trim();
    const summary = item.summary.trim();
    const text = detail || summary;
    const metadata = item.metadata;

    const kind = item.kind as string;

    switch (kind) {
      case 'user_message':
      case 'status':
      case 'error':
      case 'context_compaction':
        break;

      case 'thinking': {
        if (!text) break;
        blocks.push({
          kind: 'thinking',
          id: item.id || nextBlockId('think'),
          text,
          streaming: false,
          status: 'done',
        });
        break;
      }

      case 'agent_message': {
        if (!text) break;
        blocks.push({
          kind: 'text',
          id: item.id || nextBlockId('text'),
          content: text,
          streaming: false,
          itemId: item.id,
        });
        break;
      }

      case 'tool_call':
      case 'file_change':
      case 'command_execution': {
        const id = toolIdFromItem(item, metadata);
        const name = toolNameFromItem(item.kind, metadata, item.summary);
        const input = toolInputFromItem(metadata);
        const status = itemStatus(item.status);
        const existingIdx = blocks.findIndex((b) => b.kind === 'tool' && b.id === id);
        if (existingIdx >= 0 && blocks[existingIdx].kind === 'tool') {
          const existing = blocks[existingIdx];
          blocks[existingIdx] = {
            ...existing,
            name: name || existing.name,
            input: input || existing.input,
            output: text || existing.output,
            status: status === 'running' ? existing.status : status,
          };
        } else {
          blocks.push({
            kind: 'tool',
            id,
            name,
            input,
            output: text || undefined,
            status,
            itemId: item.id,
          });
        }
        currentToolId = id;
        break;
      }

      default:
        break;
    }
  }

  void currentToolId;
  return finalizeTimelineBlocks(blocks);
}

export function normalizeTurnEvents(
  events: readonly RawThreadEvent[],
): NormalizedStreamEvent[] {
  const out: NormalizedStreamEvent[] = [];
  for (const ev of events) {
    const norm = normalizeDesktopStreamEvent(ev);
    if (norm) out.push(norm);
  }
  return out;
}

/** Replay assistant blocks from normalized SSE/event records for one turn. */
export function turnTimelineReplayFromNormalizedEvents(
  events: readonly NormalizedStreamEvent[],
  _turnId: string,
): TurnBlock[] {
  resetBlockIdCounter();
  let timeline = createEmptyTimelineState();
  let currentToolId: string | null = null;

  for (const norm of events) {
    if (norm.kind === 'turn_started') {
      timeline = createEmptyTimelineState();
      currentToolId = null;
      continue;
    }
    if (norm.kind === 'tool_started') {
      currentToolId = norm.id;
    }
    timeline = applyTimelineEvent(timeline, norm, { currentToolId });
    if (norm.kind === 'tool_completed' && currentToolId === norm.id) {
      currentToolId = null;
    }
  }

  return finalizeTimelineBlocks(timeline.blocks);
}

/**
 * Merge item-order spine with event-only thinking segments.
 * Items win for tool/text interleaving; events fill thinking gaps.
 */
export function mergeItemBlocksWithEventTimeline(
  itemBlocks: TurnBlock[],
  eventBlocks: TurnBlock[],
): TurnBlock[] {
  if (itemBlocks.length === 0) return eventBlocks;
  if (eventBlocks.length === 0) return itemBlocks;

  const itemHasThinking = itemBlocks.some((b) => b.kind === 'thinking');
  const eventThinking = eventBlocks.filter((b) => b.kind === 'thinking');
  if (eventThinking.length === 0 || itemHasThinking) {
    return itemBlocks;
  }

  const merged: TurnBlock[] = [];
  let thinkIdx = 0;

  for (let i = 0; i < itemBlocks.length; i++) {
    const block = itemBlocks[i];
    const prev = itemBlocks[i - 1];
    const injectThinking =
      thinkIdx < eventThinking.length &&
      (i === 0 ||
        prev?.kind === 'text' ||
        (block.kind === 'tool' && prev?.kind !== 'tool'));

    if (injectThinking) {
      merged.push(eventThinking[thinkIdx]);
      thinkIdx += 1;
    }
    merged.push(block);
  }

  while (thinkIdx < eventThinking.length) {
    merged.push(eventThinking[thinkIdx]);
    thinkIdx += 1;
  }

  return merged;
}

export function buildAssistantBlocksForTurn(
  turnId: string,
  items: readonly TurnItemRecord[],
  events: readonly RawThreadEvent[],
): AssistantBlocksReplay {
  const itemBlocks = turnTimelineReplayFromThreadItems(items, turnId);
  const normalized = normalizeTurnEvents(events);
  const eventBlocks = turnTimelineReplayFromNormalizedEvents(normalized, turnId);
  const blocks = mergeItemBlocksWithEventTimeline(itemBlocks, eventBlocks);

  // P1.1 A: items-only (no events) cannot restore Thought segments — surface a soft note.
  const thinkingIncomplete =
    itemBlocks.length > 0 &&
    events.length === 0 &&
    !blocks.some((b) => b.kind === 'thinking');

  return {
    blocks,
    thinkingIncomplete: thinkingIncomplete || undefined,
  };
}

/** Walk raw events and bucket by turn_id (from JSON or turn.started / turn.completed). */
export function partitionRawEventsByTurn(
  events: readonly RawThreadEvent[],
): Map<string, RawThreadEvent[]> {
  const byTurn = new Map<string, RawThreadEvent[]>();
  let currentTurnId: string | null = null;

  const push = (turnId: string, ev: RawThreadEvent) => {
    const list = byTurn.get(turnId);
    if (list) list.push(ev);
    else byTurn.set(turnId, [ev]);
  };

  for (const ev of events) {
    let turnId: string | undefined;
    try {
      const j = JSON.parse(ev.data) as Record<string, unknown>;
      if (j.turn_id != null) turnId = String(j.turn_id);
      else {
        const payload = j.payload as Record<string, unknown> | undefined;
        if (payload?.turn_id != null) turnId = String(payload.turn_id);
        const turn = payload?.turn as Record<string, unknown> | undefined;
        if (!turnId && turn?.id != null) turnId = String(turn.id);
      }
    } catch {
      /* keep currentTurnId */
    }

    if (ev.event === 'turn.started' && turnId) {
      currentTurnId = turnId;
    }

    const resolved = turnId ?? currentTurnId;
    if (resolved) {
      push(resolved, ev);
    }

    if (ev.event === 'turn.completed') {
      currentTurnId = null;
    }
  }

  return byTurn;
}

/** Stable turn order from durable items (first-seen turn_id). */
export function orderedTurnIdsFromItems(items: readonly TurnItemRecord[]): string[] {
  const ids: string[] = [];
  const seen = new Set<string>();
  for (const item of items) {
    if (!item.turn_id || seen.has(item.turn_id)) continue;
    seen.add(item.turn_id);
    ids.push(item.turn_id);
  }
  return ids;
}
